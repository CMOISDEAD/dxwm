use anyhow::Result;
use std::time::{Duration, Instant};
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use crate::config::{
    ALERT_HISTORY, ALERT_LINES, BACKGROUND, BORDER_FOCUSED, BORDER_WIDTH, DIMMED_FG, MARGIN,
    TITLE_BG_FOCUSED, TITLE_FG_FOCUSED, TITLE_PADDING,
};
use crate::decorations::encode_text;
use crate::prompt::LINE_PADDING;
use crate::utils::local_time;
use crate::wm::WindowManager;

const ALERT_TIMEOUT: Duration = Duration::from_secs(3);
const TYPE_DURATION: Duration = Duration::from_millis(120);
const TAG_WIDTH: usize = 6;

#[derive(Default)]
pub struct Alerts {
    pub window: Option<Window>,
    lines: Vec<Line>,
    columns: usize,
    typing: bool,
}

struct Line {
    text: String,
    created_at: Instant,
}

impl Line {
    fn new(message: &str) -> Self {
        let tm = local_time();
        let time = format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec);
        let (tag, body) = match message.strip_prefix('[').and_then(|m| m.split_once(']')) {
            Some((tag, body)) => (tag.to_uppercase(), body.trim()),
            None => (String::new(), message.trim()),
        };

        Self {
            text: format!("{} {:<TAG_WIDTH$} {}", time, tag, body),
            created_at: Instant::now(),
        }
    }

    fn typed(&self) -> String {
        let count = self.text.chars().count();
        let elapsed = self.created_at.elapsed().as_millis();
        let shown = (count as u128 * elapsed).div_ceil(TYPE_DURATION.as_millis().max(1));
        self.text.chars().take(shown as usize).collect()
    }
}

impl WindowManager {
    pub fn draw_alert(&mut self, message: String) -> Result<()> {
        let alerts = &mut self.alerts;
        let history = Duration::from_secs(ALERT_HISTORY);
        alerts
            .lines
            .retain(|line| line.created_at.elapsed() < history);
        alerts.lines.push(Line::new(&message));
        let extra = alerts.lines.len().saturating_sub(ALERT_LINES);
        alerts.lines.drain(..extra);
        alerts.typing = true;

        self.place_alerts()
    }

    fn place_alerts(&mut self) -> Result<()> {
        let area = self.monitors.current().work_area();
        let border = BORDER_WIDTH as i16;
        let margin = MARGIN as i16;
        let char_width = self.decorations.char_width();

        let space = area.width as i16 - 2 * (margin + border) - 2 * TITLE_PADDING;
        let longest = self.alerts.lines.iter().map(|l| l.text.chars().count());
        let columns = (longest.max().unwrap_or(0) as i16).clamp(1, (space / char_width).max(1));
        let width = columns * char_width + 2 * TITLE_PADDING;
        let line_height = self.decorations.font_height() + 2 * LINE_PADDING;
        let height = line_height * self.alerts.lines.len().max(1) as i16;

        let x = area.x + area.width as i16 - width - 2 * border - margin;
        let y = area.y + area.height as i16 - height - 2 * border - margin;
        let (width, height) = (width.max(1) as u16, height.max(1) as u16);
        self.alerts.columns = columns as usize;

        match self.alerts.window {
            Some(window) => {
                self.conn.configure_window(
                    window,
                    &ConfigureWindowAux::new()
                        .x(x as i32)
                        .y(y as i32)
                        .width(width as u32)
                        .height(height as u32)
                        .stack_mode(StackMode::ABOVE),
                )?;
            }
            None => {
                let window = self.conn.generate_id()?;
                self.conn.create_window(
                    COPY_DEPTH_FROM_PARENT,
                    window,
                    self.root,
                    x,
                    y,
                    width,
                    height,
                    BORDER_WIDTH as u16,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &CreateWindowAux::new()
                        .background_pixel(BACKGROUND)
                        .border_pixel(BORDER_FOCUSED)
                        .override_redirect(1)
                        .event_mask(EventMask::EXPOSURE),
                )?;
                self.conn.map_window(window)?;
                self.alerts.window = Some(window);
            }
        }

        self.redraw_alerts()
    }

    pub fn redraw_alerts(&self) -> Result<()> {
        let alerts = &self.alerts;
        let Some(window) = alerts.window else {
            return Ok(());
        };

        let decorations = &self.decorations;
        let line_height = decorations.font_height() + 2 * LINE_PADDING;
        let width =
            alerts.columns as u16 * decorations.char_width() as u16 + 2 * TITLE_PADDING as u16;
        let newest = alerts.lines.len().saturating_sub(1);

        for (row, line) in alerts.lines.iter().enumerate() {
            let y = line_height * row as i16;
            let (foreground, background, text) = if row == newest {
                (TITLE_FG_FOCUSED, TITLE_BG_FOCUSED, line.typed())
            } else {
                (DIMMED_FG, BACKGROUND, line.text.clone())
            };

            decorations.fill(
                &self.conn,
                window,
                Rectangle {
                    x: 0,
                    y,
                    width,
                    height: line_height as u16,
                },
                background,
            )?;
            decorations.draw_text(
                &self.conn,
                window,
                (TITLE_PADDING, y),
                line_height,
                (foreground, background),
                &encode_text(&text, alerts.columns),
            )?;
        }

        self.conn.flush()?;
        Ok(())
    }

    pub fn clear_alerts(&mut self) -> Result<()> {
        if let Some(window) = self.alerts.window.take() {
            self.conn.destroy_window(window)?;
            self.conn.flush()?;
        }
        Ok(())
    }

    pub fn poll_alerts(&mut self) -> Result<()> {
        let Some(newest) = self.alerts.lines.last() else {
            return Ok(());
        };
        let elapsed = newest.created_at.elapsed();

        if self.alerts.typing {
            self.alerts.typing = elapsed < TYPE_DURATION;
            self.redraw_alerts()?;
        }

        if elapsed > ALERT_TIMEOUT && !self.keybindings.is_in_submap() {
            self.clear_alerts()?;
        }
        Ok(())
    }

    pub fn restack_alerts(&mut self) -> Result<()> {
        let prompt = self.prompt.as_ref().map(|p| p.window);

        for window in self
            .overlays
            .iter()
            .copied()
            .chain(self.alerts.window)
            .chain(prompt)
        {
            self.conn.configure_window(
                window,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        }
        Ok(())
    }
}
