use anyhow::Result;
use std::thread;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Gcontext, Window, WindowClass,
};
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::config::config::{BACKGROUND, BORDER_FOCUSED, FOREGROUND, MARGIN};
use crate::wm::WindowManager;

struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[allow(dead_code)]
enum Position {
    TopRight,
    TopLeft,
    Center,
    BottomRight,
    BottomLeft,
}

pub struct Alert {
    pub window: Window,
    pub gc: Gcontext,
    pub message: String,
    pub created_at: Instant,
}

#[allow(dead_code)]
impl WindowManager {
    pub fn draw_alert(&mut self, msg: String) -> Result<()> {
        self.clear_alerts()?;

        let alert_id = self.conn.generate_id()?;
        let gc_id = self.conn.generate_id()?;
        let geom = self.position(Position::BottomRight, 200, 50);

        self.conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            alert_id,
            self.root,
            geom.x as i16,
            geom.y as i16,
            geom.width as u16,
            geom.height as u16,
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
            gc_id,
            alert_id,
            &CreateGCAux::new()
                .foreground(FOREGROUND)
                .background(BACKGROUND),
        )?;

        self.conn.map_window(alert_id)?;

        self.conn.configure_window(
            alert_id,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
        )?;

        self.conn
            .image_text8(alert_id, gc_id, 20, 30, msg.as_bytes())?;

        self.conn.flush()?;

        self.alerts.push(Alert {
            window: alert_id,
            gc: gc_id,
            message: msg,
            created_at: Instant::now(),
        });

        thread::spawn(move || {
            thread::sleep(Duration::from_secs(3));
        });

        Ok(())
    }

    pub fn clear_alerts(&mut self) -> Result<()> {
        for alert in &self.alerts {
            self.conn.free_gc(alert.gc).ok();
            self.conn.destroy_window(alert.window).ok();
        }
        self.alerts.clear();
        self.conn.flush()?;
        Ok(())
    }

    pub fn clear_old_alerts(&mut self) -> Result<()> {
        let timeout = Duration::from_secs(3);

        if self.keybindings.is_in_submap() {
            return Ok(());
        }

        self.alerts.retain(|alert| {
            if alert.created_at.elapsed() > timeout {
                self.conn.free_gc(alert.gc).ok();
                self.conn.destroy_window(alert.window).ok();
                false
            } else {
                true
            }
        });

        self.conn.flush()?;
        Ok(())
    }

    pub fn restack_alerts(&mut self) -> Result<()> {
        for alert in &self.alerts {
            self.conn.configure_window(
                alert.window,
                &x11rb::protocol::xproto::ConfigureWindowAux::new()
                    .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
            )?;
        }
        self.conn.flush()?;
        Ok(())
    }

    pub fn redraw_alert(&self, alert: &Alert) -> Result<()> {
        self.conn
            .image_text8(alert.window, alert.gc, 20, 30, alert.message.as_bytes())?;
        self.conn.flush()?;
        Ok(())
    }

    fn position(&self, pos: Position, win_w: u32, win_h: u32) -> Rect {
        let monitor = self.monitors.current();
        let margin = MARGIN;

        let (relative_x, relative_y) = match pos {
            Position::TopLeft => (margin, margin),
            Position::TopRight => {
                let x = monitor.width.saturating_sub(win_w as u16 + margin as u16);
                (x as u32, margin)
            }
            Position::Center => {
                let x = monitor.width.saturating_sub(win_w as u16) / 2;
                let y = monitor.height.saturating_sub(win_h as u16) / 2;
                (x as u32, y as u32)
            }
            Position::BottomLeft => {
                let y = monitor.height.saturating_sub(win_h as u16 + margin as u16);
                (margin, y as u32)
            }
            Position::BottomRight => {
                let x = monitor.width.saturating_sub(win_w as u16 + margin as u16);
                let y = monitor.height.saturating_sub(win_h as u16 + margin as u16);
                (x as u32, y as u32)
            }
        };

        Rect {
            x: monitor.x as u32 + relative_x,
            y: monitor.y as u32 + relative_y,
            width: win_w,
            height: win_h,
        }
    }
}
