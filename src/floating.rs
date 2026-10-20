use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::properties::WmSizeHints;
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

    pub fn initial_float_rect(&self, window: Window) -> Result<Option<Rect>> {
        let hints = WmSizeHints::get_normal_hints(&self.conn, window)?
            .reply()
            .ok()
            .flatten();

        let fixed_size = hints.is_some_and(|h| h.min_size.is_some() && h.min_size == h.max_size);

        if !(fixed_size || self.is_transient(window)? || self.has_floating_type(window)?) {
            return Ok(None);
        }

        let geometry = self.conn.get_geometry(window)?.reply()?;
        let width = geometry.width as u32 + 2 * BORDER_WIDTH;
        let height = geometry.height as u32 + 2 * BORDER_WIDTH + TITLE_HEIGHT;
        let mut rect = self.centered_rect(width, height);

        if hints.is_some_and(|h| h.position.is_some()) {
            rect.x = geometry.x;
            rect.y = geometry.y;
        }

        Ok(Some(rect))
    }

    fn centered_rect(&self, width: u32, height: u32) -> Rect {
        let monitor = self.monitors.current().geometry;
        let width = width.min(monitor.width as u32) as u16;
        let height = height.min(monitor.height as u32) as u16;

        Rect {
            x: monitor.x + ((monitor.width - width) / 2) as i16,
            y: monitor.y + ((monitor.height - height) / 2) as i16,
            width,
            height,
        }
    }

    fn is_transient(&self, window: Window) -> Result<bool> {
        let reply = self
            .conn
            .get_property(
                false,
                window,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                0,
                1,
            )?
            .reply()?;

        Ok(reply
            .value32()
            .is_some_and(|mut parents| parents.any(|w| w != x11rb::NONE)))
    }

    pub fn has_window_type(&self, window: Window, types: &[Atom]) -> Result<bool> {
        let reply = self
            .conn
            .get_property(
                false,
                window,
                self.atoms.net_wm_window_type,
                AtomEnum::ATOM,
                0,
                1024,
            )?
            .reply()?;

        Ok(reply
            .value32()
            .is_some_and(|mut found| found.any(|t| types.contains(&t))))
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
            rect.width = (rect.width as i32 + dx).clamp(MIN_SIZE, u16::MAX as i32) as u16;
            rect.height = (rect.height as i32 + dy).clamp(MIN_SIZE, u16::MAX as i32) as u16;
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
