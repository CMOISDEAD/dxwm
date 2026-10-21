use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::properties::{WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::xproto::*;

use crate::clients::Rect;
use crate::config::{BORDER_WIDTH, TITLE_HEIGHT};
use crate::keyboard::lock_masks;
use crate::wm::WindowManager;

const MIN_SIZE: i32 = 100;

pub struct Drag {
    window: Window,
    resize: bool,
    pointer: (i16, i16),
    rect: Rect,
    hints: Option<WmSizeHints>,
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

    fn centered_rect(&self, width: u32, height: u32) -> Rect {
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
        let Some((monitor_id, _)) = self.find_client(window) else {
            return Ok(());
        };

        let monitor = self.monitors.current().geometry;
        let rect = self.centered_rect(monitor.width as u32 * 2 / 3, monitor.height as u32 * 2 / 3);

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

        let (window, fullscreen) = (client.window, client.fullscreen);
        let Some(rect) = client.floating.filter(|_| !fullscreen) else {
            return Ok(());
        };

        self.drag = Some(Drag {
            window,
            resize: e.detail == u8::from(ButtonIndex::M3),
            pointer: (e.root_x, e.root_y),
            rect,
            hints: self.read_size_hints(window)?,
        });

        self.set_focused_client(window)
    }

    pub fn handle_button_release(&mut self) -> Result<()> {
        self.drag = None;
        Ok(())
    }

    pub fn handle_drag_motion(&mut self, e: &MotionNotifyEvent) -> Result<bool> {
        let Some(drag) = &self.drag else {
            return Ok(false);
        };

        let dx = e.root_x as i32 - drag.pointer.0 as i32;
        let dy = e.root_y as i32 - drag.pointer.1 as i32;

        let mut rect = drag.rect;
        if drag.resize {
            let mut width = (rect.width as i32 + dx).max(MIN_SIZE);
            let mut height = (rect.height as i32 + dy).max(MIN_SIZE);
            if let Some(hints) = &drag.hints {
                let (w, h) = constrain_size(hints, width - DECORATION.0, height - DECORATION.1);
                (width, height) = (w + DECORATION.0, h + DECORATION.1);
            }
            rect.width = width.min(u16::MAX as i32) as u16;
            rect.height = height.min(u16::MAX as i32) as u16;
        } else {
            rect.x = (rect.x as i32 + dx).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            rect.y = (rect.y as i32 + dy).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }

        let window = drag.window;
        let Some((monitor_id, _)) = self.find_client(window) else {
            self.drag = None;
            return Ok(true);
        };

        if let Some(client) = self.client_mut(window) {
            client.floating = Some(rect);
        }

        self.layout_monitor(monitor_id)?;
        Ok(true)
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
