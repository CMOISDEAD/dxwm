use anyhow::Result;
use std::time::{Duration, Instant};
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use crate::config::{BACKGROUND, BORDER_FOCUSED, FOREGROUND, MARGIN};
use crate::wm::WindowManager;

const ALERT_WIDTH: u16 = 200;
const ALERT_HEIGHT: u16 = 50;
const ALERT_TIMEOUT: Duration = Duration::from_secs(3);

/// Small override-redirect window in the bottom right corner of the current monitor
pub struct Alert {
    pub window: Window,
    pub gc: Gcontext,
    pub message: String,
    pub created_at: Instant,
}

impl WindowManager {
    /// Show a message, replacing the previous alert
    pub fn draw_alert(&mut self, message: String) -> Result<()> {
        self.clear_alerts()?;

        let window = self.conn.generate_id()?;
        let gc = self.conn.generate_id()?;

        let monitor = self.monitors.current();
        let margin = MARGIN as i16;
        let x = monitor.x + monitor.width as i16 - ALERT_WIDTH as i16 - margin;
        let y = monitor.y + monitor.height as i16 - ALERT_HEIGHT as i16 - margin;

        self.conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            self.root,
            x,
            y,
            ALERT_WIDTH,
            ALERT_HEIGHT,
            1,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(BACKGROUND)
                .border_pixel(BORDER_FOCUSED)
                .override_redirect(1)
                .event_mask(EventMask::EXPOSURE),
        )?;

        self.conn.create_gc(
            gc,
            window,
            &CreateGCAux::new()
                .foreground(FOREGROUND)
                .background(BACKGROUND),
        )?;

        self.conn.map_window(window)?;

        let alert = Alert {
            window,
            gc,
            message,
            created_at: Instant::now(),
        };
        self.redraw_alert(&alert)?;
        self.alerts.push(alert);

        Ok(())
    }

    pub fn redraw_alert(&self, alert: &Alert) -> Result<()> {
        self.conn
            .image_text8(alert.window, alert.gc, 20, 30, alert.message.as_bytes())?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn clear_alerts(&mut self) -> Result<()> {
        for alert in self.alerts.drain(..) {
            self.conn.free_gc(alert.gc)?;
            self.conn.destroy_window(alert.window)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Remove expired alerts. They stay while a submap is active, so the mode is visible
    pub fn clear_old_alerts(&mut self) -> Result<()> {
        if self.keybindings.is_in_submap()
            || !self
                .alerts
                .iter()
                .any(|a| a.created_at.elapsed() > ALERT_TIMEOUT)
        {
            return Ok(());
        }

        self.clear_alerts()
    }

    /// Keep the overlays (notifications, menus...) and the alerts above clients
    /// that were just raised
    pub fn restack_alerts(&mut self) -> Result<()> {
        let alerts = self.alerts.iter().map(|a| a.window);

        for window in self.overlays.iter().copied().chain(alerts) {
            self.conn.configure_window(
                window,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        }
        Ok(())
    }
}
