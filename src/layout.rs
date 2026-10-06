use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;

use crate::clients::{Client, Rect};
use crate::config::{
    BORDER_WIDTH, COLUMN_WIDTH_PRESETS, DEFAULT_COLUMN_WIDTH, MARGIN, MIN_COLUMN_WIDTH,
    TITLE_HEIGHT,
};
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

    /// Arrange the visible workspace of a monitor. Tiled clients are columns of a
    /// horizontal strip, each with its own width, scrolled just enough to show the
    /// focused one completely. Columns out of view are parked off-screen (kept
    /// mapped, so scrolling is instant) and the ones cut by the monitor edge are
    /// clipped. Fullscreen clients cover the monitor and floating ones keep their
    /// own geometry on top
    pub fn layout_monitor(&mut self, monitor_id: usize) -> Result<()> {
        let Some(monitor) = self.monitors.get(monitor_id) else {
            return Ok(());
        };

        let (mon_x, mon_y) = (monitor.x as i32, monitor.y as i32);
        let (mon_width, mon_height) = (monitor.width as i32, monitor.height as i32);

        let margin = MARGIN as i32;
        let (y, height) = (mon_y + margin, mon_height - margin * 2);
        // Width of the strip that is visible at once
        let view = mon_width - margin * 2;

        let workspace = monitor.workspaces.current();
        let is_tiled = |c: &&Client| !c.fullscreen && c.floating.is_none();

        // Position of every column in the strip, separated by `margin`. A fraction
        // of 0.5 gives two columns that fit exactly in the view
        let mut columns: Vec<(Window, i32, i32)> = Vec::new();
        let mut end = 0;
        for client in workspace.clients.iter().filter(is_tiled) {
            let width = (client.column_width * (mon_width - margin) as f32).round() as i32 - margin;
            columns.push((client.window, end, width.max(1)));
            end += width.max(1) + margin;
        }

        // The column to keep in view: the focused client if it's tiled. Otherwise
        // the closest tiled one before it, which is the one that opened a dialog
        let anchor = workspace
            .focused_client
            .and_then(|w| workspace.position(w))
            .and_then(|idx| workspace.clients[..=idx].iter().rev().find(is_tiled))
            .map(|c| c.window);

        let mut scroll = workspace.scroll;
        if let Some(&(_, start, width)) = columns.iter().find(|c| Some(c.0) == anchor) {
            if start < scroll || width >= view {
                scroll = start;
            } else if start + width > scroll + view {
                scroll = start + width - view;
            }
        }
        // Don't leave empty space at the end of the strip after closing clients
        scroll = scroll.clamp(0, (end - margin - view).max(0));

        let clients: Vec<(Window, Window, bool, Option<Rect>)> = workspace
            .clients
            .iter()
            .map(|c| (c.window, c.frame, c.fullscreen, c.floating))
            .collect();

        self.monitors.monitors[monitor_id]
            .workspaces
            .current_mut()
            .scroll = scroll;

        for (window, frame, fullscreen, floating) in clients {
            if fullscreen {
                self.configure_client(window, mon_x, mon_y, mon_width, mon_height, false, None)?;
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

                self.configure_client(window, x, y, width, height, true, None)?;
            } else if let Some(&(_, start, width)) = columns.iter().find(|c| c.0 == window) {
                let x = mon_x + margin + start - scroll;
                let visible = (x.max(mon_x), (x + width).min(mon_x + mon_width));

                if visible.0 >= visible.1 {
                    // Monitors never have negative coordinates, so this is outside all of them
                    self.configure_client(window, -2 * mon_width, y, width, height, true, None)?;
                } else if visible == (x, x + width) {
                    self.configure_client(window, x, y, width, height, true, None)?;
                } else {
                    // The rest would be drawn on the next monitor
                    self.configure_client(window, x, y, width, height, true, Some(visible))?;
                }
            }
        }

        self.raise_floating(monitor_id)?;
        self.restack_alerts()?;
        self.ignore_pending_enters()?;
        self.conn.flush()?;
        Ok(())
    }

    /// Set the width of the focused column with `change`, which gets the current
    /// fraction and returns the new one
    fn set_column_width(&mut self, change: impl Fn(f32) -> f32) -> Result<()> {
        let Some(window) = self.focused_client() else {
            return Ok(());
        };

        let Some(client) = self.client_mut(window) else {
            return Ok(());
        };

        if client.fullscreen || client.floating.is_some() {
            return Ok(());
        }

        client.column_width = change(client.column_width).clamp(MIN_COLUMN_WIDTH, 1.0);
        let percent = (client.column_width * 100.0).round();

        self.layout()?;
        self.draw_alert(format!("[WIDTH] {}%", percent))
    }

    /// Make the focused column narrower (negative) or wider (positive)
    pub fn resize_column(&mut self, delta: f32) -> Result<()> {
        self.set_column_width(|width| width + delta)
    }

    /// Switch the focused column to the next preset width, wrapping around
    pub fn cycle_column_width(&mut self) -> Result<()> {
        self.set_column_width(|width| {
            COLUMN_WIDTH_PRESETS
                .into_iter()
                .find(|&preset| preset > width + 0.01)
                .unwrap_or(COLUMN_WIDTH_PRESETS[0])
        })
    }

    /// Switch the focused column between the whole view and the default width
    pub fn toggle_maximize_column(&mut self) -> Result<()> {
        self.set_column_width(|width| {
            if width < 1.0 {
                1.0
            } else {
                DEFAULT_COLUMN_WIDTH
            }
        })
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
    /// `height` include the border. Undecorated clients (fullscreen) fill the frame.
    /// `clip` is the horizontal range (root coordinates) of the frame to show
    #[allow(clippy::too_many_arguments)]
    fn configure_client(
        &mut self,
        window: Window,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        decorated: bool,
        clip: Option<(i32, i32)>,
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

        match clip {
            Some((start, end)) => {
                // Shape coordinates are relative to the frame's origin, inside its border
                let rectangle = Rectangle {
                    x: (start - x - border as i32) as i16,
                    y: -(border as i16),
                    width: (end - start) as u16,
                    height: height.clamp(1, u16::MAX as i32) as u16,
                };
                self.conn.shape_rectangles(
                    shape::SO::SET,
                    shape::SK::BOUNDING,
                    ClipOrdering::UNSORTED,
                    frame,
                    0,
                    0,
                    &[rectangle],
                )?;
            }
            None => {
                self.conn.shape_mask(
                    shape::SO::SET,
                    shape::SK::BOUNDING,
                    frame,
                    0,
                    0,
                    x11rb::NONE,
                )?;
            }
        }

        if let Some(client) = self.client_mut(window) {
            client.x = x as i16;
            client.y = y as i16;
            client.width = width as u16;
            client.height = height as u16;
        }

        Ok(())
    }
}
