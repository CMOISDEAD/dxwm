use anyhow::Result;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Gcontext, Window, WindowClass,
};
use x11rb::COPY_DEPTH_FROM_PARENT;

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

impl WindowManager {
    pub fn draw_alert(&mut self, msg: String) -> Result<()> {
        self.clear_alerts()?;

        let alert_id = self.conn.generate_id()?;
        let gc_id = self.conn.generate_id()?;
        let rect = self.position(Position::BottomRight, 200, 50);

        self.conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            alert_id,
            self.root,
            rect.x as i16,
            rect.y as i16,
            rect.width as u16,
            rect.height as u16,
            1,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0x000000)
                .border_pixel(0x0000ff)
                .override_redirect(1)
                .event_mask(EventMask::EXPOSURE),
        )?;

        self.conn
            .create_gc(gc_id, alert_id, &CreateGCAux::new().foreground(0xffffff))?;

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
        let mut rect = Rect {
            x: 0,
            y: 0,
            width: win_w,
            height: win_h,
        };
        let screen_geom = self.conn.setup().roots.get(0).unwrap();
        let margin = 10;

        match pos {
            Position::TopLeft => {
                rect.x = margin;
                rect.y = margin;
            }
            Position::TopRight => {
                rect.x = screen_geom.width_in_pixels as u32 - (margin + win_w);
                rect.y = margin;
            }
            Position::Center => {
                rect.x = ((screen_geom.width_in_pixels.saturating_sub(win_w as u16)) / 2) as u32;
                rect.y = ((screen_geom.height_in_pixels.saturating_sub(win_h as u16)) / 2) as u32;
            }
            Position::BottomLeft => {
                rect.x = margin;
                rect.y = screen_geom.height_in_pixels as u32 - (margin as u32 + win_h);
            }
            Position::BottomRight => {
                rect.x = screen_geom.width_in_pixels as u32 - (margin as u32 + win_w);
                rect.y = screen_geom.height_in_pixels as u32 - (margin as u32 + win_h);
            }
        };

        rect
    }
}
