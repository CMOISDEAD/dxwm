use std::time::{Duration, Instant};

use anyhow::Result;
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use crate::config::{
    BACKGROUND, BORDER_FOCUSED, BORDER_UNFOCUSED, BORDER_WIDTH, DIMMED_FG, FOREGROUND, MARGIN,
    MINIMAP_VIEW, SELECTED_BG, SELECTED_FG, SHOW_BAR, TITLE_PADDING,
};
use crate::decorations::encode_text;
use crate::layout::strip;
use crate::prompt::LINE_PADDING;
use crate::utils::{battery, local_time};
use crate::wm::WindowManager;

const STATUS_INTERVAL: Duration = Duration::from_secs(5);
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Default)]
pub struct Bars {
    windows: Vec<Window>,
    drawn: Vec<Option<Modeline>>,
    status: String,
    updated: Option<Instant>,
}

impl Bars {
    pub fn contains(&self, window: Window) -> bool {
        self.windows.contains(&window)
    }
}

#[derive(Clone, PartialEq)]
struct Modeline {
    width: u16,
    active: bool,
    workspaces: Vec<WorkspaceView>,
    status: String,
}

#[derive(Clone, PartialEq)]
struct WorkspaceView {
    number: usize,
    visible: bool,
    urgent: bool,
    columns: Vec<(i16, i16, bool)>,
    view: Option<i16>,
}

impl WorkspaceView {
    fn width(&self) -> i16 {
        let columns = self.columns.last().map_or(0, |&(x, width, _)| x + width);
        columns.max(self.view.map_or(0, |x| x + MINIMAP_VIEW as i16))
    }
}

fn status() -> String {
    let tm = local_time();
    let date = format!(
        "{} {:02} {} {:02}:{:02}",
        DAYS[tm.tm_wday as usize % 7],
        tm.tm_mday,
        MONTHS[tm.tm_mon as usize % 12],
        tm.tm_hour,
        tm.tm_min
    );

    match battery() {
        Some((label, capacity)) => format!("{} {}%  {}", label, capacity, date),
        None => date,
    }
}

impl WindowManager {
    fn line_height(&self) -> i16 {
        self.decorations.font_height() + 2 * LINE_PADDING
    }

    pub fn place_bars(&mut self) -> Result<()> {
        if !SHOW_BAR {
            return Ok(());
        }

        let count = self.monitors.count();
        while self.bars.windows.len() > count {
            if let Some(window) = self.bars.windows.pop() {
                self.conn.destroy_window(window)?;
            }
        }

        let border = BORDER_WIDTH as i16;
        let margin = MARGIN as i16;
        let height = self.line_height();

        for monitor_id in 0..count {
            let area = self.monitors.monitors[monitor_id].work_area();
            let x = area.x + margin;
            let y = area.y + area.height as i16 - margin - height - 2 * border;
            let width = (area.width as i16 - 2 * (margin + border)).max(1);

            match self.bars.windows.get(monitor_id) {
                Some(&window) => {
                    self.conn.configure_window(
                        window,
                        &ConfigureWindowAux::new()
                            .x(x as i32)
                            .y(y as i32)
                            .width(width as u32)
                            .height(height as u32),
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
                        width as u16,
                        height as u16,
                        BORDER_WIDTH as u16,
                        WindowClass::INPUT_OUTPUT,
                        0,
                        &CreateWindowAux::new()
                            .background_pixel(BACKGROUND)
                            .border_pixel(BORDER_FOCUSED)
                            .override_redirect(1)
                            .event_mask(EventMask::EXPOSURE),
                    )?;
                    self.conn.configure_window(
                        window,
                        &ConfigureWindowAux::new().stack_mode(StackMode::BELOW),
                    )?;
                    self.conn.map_window(window)?;
                    self.bars.windows.push(window);
                }
            }
        }

        if self.bars.updated.is_none() {
            self.bars.status = status();
            self.bars.updated = Some(Instant::now());
        }

        self.bars.drawn = vec![None; count];
        self.update_bars()
    }

    pub fn bars_timeout(&self) -> Option<Duration> {
        let elapsed = self.bars.updated.map_or(STATUS_INTERVAL, |t| t.elapsed());
        SHOW_BAR.then(|| STATUS_INTERVAL.saturating_sub(elapsed))
    }

    pub fn poll_bars(&mut self) -> Result<()> {
        if self
            .bars
            .updated
            .is_some_and(|t| t.elapsed() < STATUS_INTERVAL)
        {
            return Ok(());
        }

        self.bars.status = status();
        self.bars.updated = Some(Instant::now());
        self.update_bars()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn redraw_bar(&mut self, window: Window) -> Result<()> {
        if let Some(index) = self.bars.windows.iter().position(|&w| w == window) {
            self.bars.drawn[index] = None;
        }
        self.update_bars()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn update_bars(&mut self) -> Result<()> {
        for index in 0..self.bars.windows.len() {
            let modeline = self.modeline(index);
            if self.bars.drawn.get(index) == Some(&modeline) {
                continue;
            }

            if let Some(modeline) = &modeline {
                self.draw_modeline(self.bars.windows[index], modeline)?;
            }
            if let Some(drawn) = self.bars.drawn.get_mut(index) {
                *drawn = modeline;
            }
        }
        Ok(())
    }

    fn modeline(&self, monitor_id: usize) -> Option<Modeline> {
        let monitor = self.monitors.get(monitor_id)?;
        let area = monitor.work_area();
        let width = area.width as i32;
        let view = (width - 2 * MARGIN as i32).max(1);
        let scale =
            |pixels: i32| (pixels as f32 * MINIMAP_VIEW as f32 / view as f32).round() as i16;

        let workspaces = monitor
            .workspaces
            .workspaces
            .iter()
            .filter_map(|ws| {
                let visible = ws.id == monitor.workspaces.current_workspace;
                if ws.clients.is_empty() && !visible {
                    return None;
                }

                let columns = strip(ws, width)
                    .into_iter()
                    .map(|column| {
                        let x = scale(column.start);
                        let end = scale(column.start + column.width);
                        let focused = ws
                            .focused_client
                            .is_some_and(|w| column.windows.contains(&w));
                        (x, (end - x - 1).max(1), focused)
                    })
                    .collect();

                Some(WorkspaceView {
                    number: ws.number(),
                    visible,
                    urgent: ws.clients.iter().any(|c| c.urgent),
                    view: (visible && !ws.clients.is_empty()).then(|| scale(ws.scroll)),
                    columns,
                })
            })
            .collect();

        Some(Modeline {
            width: (width - 2 * (MARGIN + BORDER_WIDTH) as i32).max(1) as u16,
            active: monitor_id == self.monitors.current_monitor,
            workspaces,
            status: self.bars.status.clone(),
        })
    }

    fn draw_modeline(&self, window: Window, modeline: &Modeline) -> Result<()> {
        let conn = &self.conn;
        let decorations = &self.decorations;
        let char_width = decorations.char_width();
        let height = self.line_height();
        let row = |x: i16, width: i16| Rectangle {
            x,
            y: 0,
            width: width.max(0) as u16,
            height: height as u16,
        };

        decorations.fill(conn, window, row(0, modeline.width as i16), BACKGROUND)?;

        let map_height = (decorations.font_height() - 4).max(3);
        let map_y = (height - map_height) / 2;
        let mut x = TITLE_PADDING;

        for workspace in &modeline.workspaces {
            let mark = if workspace.urgent { '*' } else { ' ' };
            let label = format!(" {}{}", workspace.number, mark);
            let label_width = label.len() as i16 * char_width;
            let (foreground, background) = match (workspace.visible, modeline.active) {
                (true, true) => (SELECTED_FG, SELECTED_BG),
                (true, false) => (SELECTED_FG, BORDER_UNFOCUSED),
                (false, _) => (FOREGROUND, BACKGROUND),
            };

            decorations.fill(conn, window, row(x, label_width), background)?;
            decorations.draw_text(
                conn,
                window,
                (x, 0),
                height,
                (foreground, background),
                &encode_text(&label, label.len()),
            )?;
            x += label_width + char_width;

            if let Some(view) = workspace.view {
                decorations.outline(
                    conn,
                    window,
                    Rectangle {
                        x: x + view - 2,
                        y: map_y - 2,
                        width: MINIMAP_VIEW as u16 + 2,
                        height: map_height as u16 + 3,
                    },
                    DIMMED_FG,
                )?;
            }

            for &(column_x, column_width, focused) in &workspace.columns {
                let color = if focused { FOREGROUND } else { DIMMED_FG };
                decorations.fill(
                    conn,
                    window,
                    Rectangle {
                        x: x + column_x,
                        y: map_y,
                        width: column_width as u16,
                        height: map_height as u16,
                    },
                    color,
                )?;
            }

            if workspace.width() > 0 {
                x += workspace.width() + 2 * char_width;
            }
        }

        let status_width = modeline.status.chars().count() as i16 * char_width;
        let status_x = modeline.width as i16 - TITLE_PADDING - status_width;
        let dashes = ((status_x - char_width - x) / char_width).max(0) as usize;

        decorations.draw_text(
            conn,
            window,
            (x, 0),
            height,
            (DIMMED_FG, BACKGROUND),
            &encode_text(&"-".repeat(dashes), dashes),
        )?;
        decorations.draw_text(
            conn,
            window,
            (status_x, 0),
            height,
            (FOREGROUND, BACKGROUND),
            &encode_text(&modeline.status, modeline.status.chars().count()),
        )?;

        Ok(())
    }
}
