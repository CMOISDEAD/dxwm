use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use crate::config::{BORDER_WIDTH, MARGIN, TITLE_HEIGHT};
use crate::wm::WindowManager;

impl WindowManager {
    /// Arrange the visible workspace of the current monitor
    pub fn layout(&mut self) -> Result<()> {
        self.layout_monitor(self.monitors.current_monitor)
    }

    pub fn layout_all_monitors(&mut self) -> Result<()> {
        for monitor_id in 0..self.monitors.count() {
            self.layout_monitor(monitor_id)?;
        }
        Ok(())
    }

    /// Arrange the visible workspace of a monitor: the focused client fills the
    /// monitor, fullscreen clients cover it and the rest are parked off-screen
    /// (kept mapped, so switching between them is instant)
    pub fn layout_monitor(&mut self, monitor_id: usize) -> Result<()> {
        let Some(monitor) = self.monitors.get(monitor_id) else {
            return Ok(());
        };

        let (mon_x, mon_y) = (monitor.x as i32, monitor.y as i32);
        let (mon_width, mon_height) = (monitor.width as i32, monitor.height as i32);

        let margin = MARGIN as i32;
        let (x, y) = (mon_x + margin, mon_y + margin);
        let (width, height) = (mon_width - margin * 2, mon_height - margin * 2);

        let workspace = monitor.workspaces.current();

        // A fullscreen focused client covers everything, show the first tiled one below
        let shown = workspace
            .focused_client
            .filter(|&w| workspace.get(w).is_some_and(|c| !c.fullscreen))
            .or_else(|| {
                workspace
                    .clients
                    .iter()
                    .find(|c| !c.fullscreen)
                    .map(|c| c.window)
            });

        let clients: Vec<(Window, Window, bool)> = workspace
            .clients
            .iter()
            .map(|c| (c.window, c.frame, c.fullscreen))
            .collect();

        for (window, frame, fullscreen) in clients {
            if fullscreen {
                self.configure_client(window, mon_x, mon_y, mon_width, mon_height, false)?;
                self.conn.configure_window(
                    frame,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )?;
            } else if Some(window) == shown {
                self.configure_client(window, x, y, width, height, true)?;
            } else {
                // Monitors never have negative coordinates, so this is outside all of them
                self.configure_client(window, -2 * mon_width, y, width, height, true)?;
            }
        }

        self.restack_alerts()?;
        self.ignore_pending_enters()?;
        self.conn.flush()?;
        Ok(())
    }

    /// Move and resize a client's frame, and the client inside it. `width` and
    /// `height` include the border. Undecorated clients (fullscreen) fill the frame
    fn configure_client(
        &mut self,
        window: Window,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        decorated: bool,
    ) -> Result<()> {
        let Some(frame) = self.client(window).map(|c| c.frame) else {
            return Ok(());
        };

        let (border, title) = if decorated {
            (BORDER_WIDTH, TITLE_HEIGHT)
        } else {
            (0, 0)
        };

        let x = x.clamp(i16::MIN as i32, i16::MAX as i32);
        let y = y.clamp(i16::MIN as i32, i16::MAX as i32);
        let inner_width = (width - 2 * border as i32).clamp(1, u16::MAX as i32);
        let inner_height = (height - 2 * border as i32).clamp(1, u16::MAX as i32);

        self.conn.configure_window(
            frame,
            &ConfigureWindowAux::new()
                .x(x)
                .y(y)
                .width(inner_width as u32)
                .height(inner_height as u32)
                .border_width(border),
        )?;

        self.conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(0)
                .y(title as i32)
                .width(inner_width as u32)
                .height((inner_height - title as i32).max(1) as u32)
                .border_width(0),
        )?;

        if let Some(client) = self.client_mut(window) {
            client.x = x as i16;
            client.y = y as i16;
            client.width = width as u16;
            client.height = height as u16;
        }

        Ok(())
    }
}
