use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::properties::{WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::xproto::*;

use crate::clients::Rect;
use crate::config::{BORDER_WIDTH, MARGIN, MIN_COLUMN_WIDTH, TITLE_HEIGHT};
use crate::keyboard::lock_masks;
use crate::layout::strip;
use crate::wm::WindowManager;

const MIN_SIZE: i32 = 100;

#[derive(Clone, Copy)]
pub enum Drag {
    Floating {
        window: Window,
        resize: bool,
        pointer: (i16, i16),
        rect: Rect,
        hints: Option<WmSizeHints>,
    },
    Column {
        monitor_id: usize,
        head: Window,
        pointer: i16,
        width: i32,
        area_width: i32,
    },
}

const DECORATION: (i32, i32) = (
    2 * BORDER_WIDTH as i32,
    (2 * BORDER_WIDTH + TITLE_HEIGHT) as i32,
);

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let center = |start: i16, size: u16, inner: u16| {
        (start as i32 + (size as i32 - inner as i32) / 2).clamp(i16::MIN as i32, i16::MAX as i32)
            as i16
    };

    Rect {
        x: center(area.x, area.width, width),
        y: center(area.y, area.height, height),
        width,
        height,
    }
}

/// ICCCM: the client's size snaps to base + n * increment and stays within min/max
fn constrain_size(hints: &WmSizeHints, width: i32, height: i32) -> (i32, i32) {
    let base = hints.base_size.or(hints.min_size).unwrap_or((0, 0));
    let min = hints.min_size.or(hints.base_size).unwrap_or((1, 1));
    let increment = hints.size_increment.unwrap_or((1, 1));
    let max = hints.max_size.unwrap_or((0, 0));

    let axis = |size: i32, base: i32, increment: i32, min: i32, max: i32| {
        let mut size = size;
        if increment > 1 {
            size -= (size - base).rem_euclid(increment);
        }
        size = size.max(min);
        if max > 0 {
            size = size.min(max);
        }
        size.max(1)
    };

    (
        axis(width, base.0, increment.0, min.0, max.0),
        axis(height, base.1, increment.1, min.1, max.1),
    )
}

impl WindowManager {
    pub fn grab_buttons(&self) -> Result<()> {
        for button in [
            ButtonIndex::M1,
            ButtonIndex::M3,
            ButtonIndex::M4,
            ButtonIndex::M5,
        ] {
            for extra in lock_masks() {
                self.conn.grab_button(
                    false,
                    self.root,
                    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::BUTTON_MOTION,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    button,
                    ModMask::M4 | extra,
                )?;
            }
        }
        Ok(())
    }

    pub fn initial_float_rect(&self, window: Window, force: Option<bool>) -> Result<Option<Rect>> {
        let hints = self.read_size_hints(window)?;
        let parent = self.transient_for(window)?;

        let fixed_size = hints.is_some_and(|h| h.min_size.is_some() && h.min_size == h.max_size);

        let floating = match force {
            Some(floating) => floating,
            None => fixed_size || parent.is_some() || self.has_floating_type(window)?,
        };
        if !floating {
            return Ok(None);
        }

        let geometry = self.conn.get_geometry(window)?.reply()?;
        let width = geometry.width as u32 + DECORATION.0 as u32;
        let height = geometry.height as u32 + DECORATION.1 as u32;

        let monitor = self.monitors.current().geometry;
        let parent = parent
            .and_then(|w| self.client(w))
            .map(|c| c.geometry)
            .filter(|g| {
                let (x, y) = g.center();
                monitor.contains(x, y)
            });
        let mut rect = match parent {
            Some(parent) => centered(parent, width as u16, height as u16),
            None => self.centered_rect(width, height),
        };

        let position = hints.and_then(|h| h.position);
        let user_position = matches!(
            position,
            Some((WmSizeHintsSpecification::UserSpecified, ..))
        );
        if user_position || (parent.is_none() && position.is_some()) {
            rect.x = geometry.x;
            rect.y = geometry.y;
        }

        Ok(Some(rect))
    }

    pub fn centered_rect(&self, width: u32, height: u32) -> Rect {
        let monitor = self.monitors.current().geometry;
        let width = width.min(monitor.width as u32) as u16;
        let height = height.min(monitor.height as u32) as u16;
        centered(monitor, width, height)
    }

    fn read_size_hints(&self, window: Window) -> Result<Option<WmSizeHints>> {
        Ok(WmSizeHints::get_normal_hints(&self.conn, window)?
            .reply()
            .ok()
            .flatten())
    }

    fn transient_for(&self, window: Window) -> Result<Option<Window>> {
        Ok(self
            .property32(window, AtomEnum::WM_TRANSIENT_FOR, AtomEnum::WINDOW)?
            .first()
            .copied()
            .filter(|&parent| parent != x11rb::NONE))
    }

    pub fn has_window_type(&self, window: Window, types: &[Atom]) -> Result<bool> {
        Ok(self
            .property32(window, self.atoms.net_wm_window_type, AtomEnum::ATOM)?
            .iter()
            .any(|t| types.contains(t)))
    }

    fn has_floating_type(&self, window: Window) -> Result<bool> {
        self.has_window_type(
            window,
            &[
                self.atoms.net_wm_window_type_dialog,
                self.atoms.net_wm_window_type_utility,
                self.atoms.net_wm_window_type_splash,
                self.atoms.net_wm_window_type_notification,
            ],
        )
    }

    pub fn is_notification(&self, window: Window) -> Result<bool> {
        self.has_window_type(window, &[self.atoms.net_wm_window_type_notification])
    }

    pub fn toggle_floating(&mut self, window: Window) -> Result<()> {
        let Some((monitor_id, workspace_id)) = self.find_client(window) else {
            return Ok(());
        };

        let monitor = self.monitors.current().geometry;
        let rect = self.centered_rect(monitor.width as u32 * 2 / 3, monitor.height as u32 * 2 / 3);

        if let Some(workspace) = self.workspace_of_mut(monitor_id, workspace_id) {
            workspace.leave_column(window);
        }
        if let Some(client) = self.client_mut(window) {
            client.floating = match client.floating {
                Some(_) => None,
                None => Some(rect),
            };
        }

        self.layout_monitor(monitor_id)?;
        self.focus_current()
    }

    pub fn handle_configure_request(&mut self, e: ConfigureRequestEvent) -> Result<()> {
        let Some((monitor_id, _)) = self.find_client(e.window) else {
            self.conn
                .configure_window(e.window, &ConfigureWindowAux::from_configure_request(&e))?;
            self.conn.flush()?;
            return Ok(());
        };

        if let Some(client) = self.client_mut(e.window)
            && let Some(rect) = &mut client.floating
        {
            if e.value_mask.contains(ConfigWindow::X) {
                rect.x = e.x;
            }
            if e.value_mask.contains(ConfigWindow::Y) {
                rect.y = e.y;
            }
            if e.value_mask.contains(ConfigWindow::WIDTH) {
                rect.width = e.width + 2 * BORDER_WIDTH as u16;
            }
            if e.value_mask.contains(ConfigWindow::HEIGHT) {
                rect.height = e.height + (2 * BORDER_WIDTH + TITLE_HEIGHT) as u16;
            }

            self.layout_monitor(monitor_id)?;
        }

        self.send_configure_notify(e.window)
    }

    /// ICCCM: a client waits for this after a configure request we didn't honor
    fn send_configure_notify(&self, window: Window) -> Result<()> {
        let Some(client) = self.client(window) else {
            return Ok(());
        };

        let (border, title) = if client.fullscreen {
            (0, 0)
        } else {
            (BORDER_WIDTH as i16, TITLE_HEIGHT as i16)
        };

        let event = ConfigureNotifyEvent {
            response_type: CONFIGURE_NOTIFY_EVENT,
            sequence: 0,
            event: window,
            window,
            above_sibling: x11rb::NONE,
            x: client.geometry.x + border,
            y: client.geometry.y + border + title,
            width: client.geometry.width.saturating_sub(2 * border as u16),
            height: client
                .geometry
                .height
                .saturating_sub((2 * border + title) as u16),
            border_width: 0,
            override_redirect: false,
        };

        self.conn
            .send_event(false, window, EventMask::STRUCTURE_NOTIFY, event)?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn handle_button_press(&mut self, e: ButtonPressEvent) -> Result<()> {
        if e.detail == u8::from(ButtonIndex::M4) {
            return self.focus_prev();
        }
        if e.detail == u8::from(ButtonIndex::M5) {
            return self.focus_next();
        }

        let Some(client) = self.client_by_frame(e.child) else {
            return Ok(());
        };
        let (window, floating) = (client.window, client.floating);
        if client.fullscreen {
            return Ok(());
        }

        let resize = e.detail == u8::from(ButtonIndex::M3);
        self.drag = match floating {
            Some(rect) => Some(Drag::Floating {
                window,
                resize,
                pointer: (e.root_x, e.root_y),
                rect,
                hints: self.read_size_hints(window)?,
            }),
            None if resize => self.column_drag(window, e.root_x),
            None => None,
        };

        self.set_focused_client(window)
    }

    fn column_drag(&self, window: Window, pointer: i16) -> Option<Drag> {
        let (monitor_id, workspace_id) = self.find_client(window)?;
        let monitor = self.monitors.get(monitor_id)?;
        let workspace = monitor.workspaces.workspaces.get(workspace_id)?;
        let area_width = monitor.work_area().width as i32;
        let column = strip(workspace, area_width)
            .into_iter()
            .find(|c| c.windows.contains(&window))?;

        Some(Drag::Column {
            monitor_id,
            head: column.windows[0],
            pointer,
            width: column.width,
            area_width,
        })
    }

    pub fn handle_button_release(&mut self) -> Result<()> {
        if let Some(Drag::Column { head, .. }) = self.drag.take()
            && let Some(client) = self.client(head)
        {
            let percent = (client.column_width * 100.0).round();
            self.draw_alert(format!("[WIDTH] {}%", percent))?;
        }
        Ok(())
    }

    pub fn handle_drag_motion(&mut self, e: &MotionNotifyEvent) -> Result<bool> {
        match self.drag {
            None => return Ok(false),
            Some(Drag::Column {
                monitor_id,
                head,
                pointer,
                width,
                area_width,
            }) => {
                let width = width + e.root_x as i32 - pointer as i32;
                self.drag_column(monitor_id, head, width, area_width)?;
            }
            Some(Drag::Floating {
                window,
                resize,
                pointer,
                rect,
                hints,
            }) => {
                let dx = e.root_x as i32 - pointer.0 as i32;
                let dy = e.root_y as i32 - pointer.1 as i32;
                self.drag_floating(window, resize, (dx, dy), rect, hints)?;
            }
        }
        Ok(true)
    }

    fn drag_column(
        &mut self,
        monitor_id: usize,
        head: Window,
        width: i32,
        area_width: i32,
    ) -> Result<()> {
        let margin = MARGIN as i32;
        let fraction = (width + margin) as f32 / (area_width - margin).max(1) as f32;
        if let Some(client) = self.client_mut(head) {
            client.column_width = fraction.clamp(MIN_COLUMN_WIDTH, 1.0);
        }
        self.layout_monitor(monitor_id)
    }

    fn drag_floating(
        &mut self,
        window: Window,
        resize: bool,
        (dx, dy): (i32, i32),
        mut rect: Rect,
        hints: Option<WmSizeHints>,
    ) -> Result<()> {
        if resize {
            let mut width = (rect.width as i32 + dx).max(MIN_SIZE);
            let mut height = (rect.height as i32 + dy).max(MIN_SIZE);
            if let Some(hints) = &hints {
                let (w, h) = constrain_size(hints, width - DECORATION.0, height - DECORATION.1);
                (width, height) = (w + DECORATION.0, h + DECORATION.1);
            }
            rect.width = width.min(u16::MAX as i32) as u16;
            rect.height = height.min(u16::MAX as i32) as u16;
        } else {
            rect.x = (rect.x as i32 + dx).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            rect.y = (rect.y as i32 + dy).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }

        let Some((monitor_id, _)) = self.find_client(window) else {
            self.drag = None;
            return Ok(());
        };

        if let Some(client) = self.client_mut(window) {
            client.floating = Some(rect);
        }
        self.layout_monitor(monitor_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_snap_to_the_increments_from_the_base() {
        let hints = WmSizeHints {
            base_size: Some((4, 2)),
            size_increment: Some((10, 20)),
            ..WmSizeHints::new()
        };
        assert_eq!(constrain_size(&hints, 157, 85), (154, 82));
    }

    #[test]
    fn sizes_stay_within_min_and_max() {
        let hints = WmSizeHints {
            min_size: Some((200, 100)),
            max_size: Some((400, 300)),
            ..WmSizeHints::new()
        };
        assert_eq!(constrain_size(&hints, 50, 50), (200, 100));
        assert_eq!(constrain_size(&hints, 900, 900), (400, 300));
        assert_eq!(constrain_size(&WmSizeHints::new(), 123, 45), (123, 45));
    }

    #[test]
    fn centers_over_an_area() {
        let parent = Rect {
            x: 100,
            y: 50,
            width: 400,
            height: 300,
        };
        assert_eq!(
            centered(parent, 200, 100),
            Rect {
                x: 200,
                y: 150,
                width: 200,
                height: 100
            }
        );
        assert_eq!(centered(parent, 600, 300).x, 0);
    }
}
