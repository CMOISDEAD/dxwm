use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use crate::clients::{Client, Rect};
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

    /// Arrange the visible workspace of a monitor: one tiled client fills the
    /// monitor and the rest are parked off-screen (kept mapped, so switching
    /// between them is instant). Fullscreen clients cover the monitor and
    /// floating ones keep their own geometry on top
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
        let is_tiled = |c: &&Client| !c.fullscreen && c.floating.is_none();

        // The focused client if it's tiled. Otherwise the closest tiled one before
        // it, which is the one that opened a focused dialog
        let shown = workspace
            .focused_client
            .and_then(|w| workspace.position(w))
            .and_then(|idx| workspace.clients[..=idx].iter().rev().find(is_tiled))
            .or_else(|| workspace.clients.iter().find(is_tiled))
            .map(|c| c.window);

        let clients: Vec<(Window, Window, bool, Option<Rect>)> = workspace
            .clients
            .iter()
            .map(|c| (c.window, c.frame, c.fullscreen, c.floating))
            .collect();

        for (window, frame, fullscreen, floating) in clients {
            if fullscreen {
                self.configure_client(window, mon_x, mon_y, mon_width, mon_height, false)?;
                self.conn.configure_window(
                    frame,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )?;
            } else if let Some(rect) = floating {
                // Keep it inside its monitor
                let width = (rect.width as i32).min(mon_width);
                let height = (rect.height as i32).min(mon_height);
                let x = (rect.x as i32).clamp(mon_x, mon_x + mon_width - width);
                let y = (rect.y as i32).clamp(mon_y, mon_y + mon_height - height);

                self.configure_client(window, x, y, width, height, true)?;
            } else if Some(window) == shown {
                self.configure_client(window, x, y, width, height, true)?;
            } else {
                // Monitors never have negative coordinates, so this is outside all of them
                self.configure_client(window, -2 * mon_width, y, width, height, true)?;
            }
        }

        self.raise_floating(monitor_id)?;
        self.restack_alerts()?;
        self.ignore_pending_enters()?;
        self.conn.flush()?;
        Ok(())
    }

    /// Floating clients of the visible workspace stay above tiled and fullscreen
    /// ones, with the focused one on top
    pub fn raise_floating(&self, monitor_id: usize) -> Result<()> {
        let Some(monitor) = self.monitors.get(monitor_id) else {
            return Ok(());
        };

        let workspace = monitor.workspaces.current();
        let is_focused = |c: &Client| workspace.focused_client == Some(c.window);

        let mut floating: Vec<&Client> = workspace
            .clients
            .iter()
            .filter(|c| c.floating.is_some() && !c.fullscreen)
            .collect();
        floating.sort_by_key(|c| is_focused(c));

        for client in floating {
            self.conn.configure_window(
                client.frame,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        }

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
