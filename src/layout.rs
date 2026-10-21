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
use crate::workspaces::Workspace;

/// Frame geometry in root coordinates, border included
#[derive(Debug, PartialEq)]
struct Placement {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// Border and title bar, fullscreen clients have none
    decorated: bool,
    /// Horizontal range (root coordinates) of the frame to show
    clip: Option<(i32, i32)>,
}

impl Placement {
    fn framed(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            decorated: true,
            clip: None,
        }
    }
}

pub fn strip(workspace: &Workspace, width: i32) -> Vec<(Window, i32, i32)> {
    let margin = MARGIN as i32;
    let mut columns = Vec::new();
    let mut end = 0;

    for client in workspace.clients.iter().filter(|c| c.is_tiled()) {
        let column =
            ((client.column_width * (width - margin) as f32).round() as i32 - margin).max(1);
        columns.push((client.window, end, column));
        end += column + margin;
    }

    columns
}

/// Where the clients of `workspace` go on a monitor (`area` is its work area), and
/// the scroll that keeps the focused column in view
fn arrange(
    workspace: &Workspace,
    monitor: Rect,
    area: Rect,
) -> (i32, Vec<(Window, Window, Placement)>) {
    let (mon_x, mon_y) = (monitor.x as i32, monitor.y as i32);
    let (mon_width, mon_height) = (monitor.width as i32, monitor.height as i32);
    let (area_x, area_width) = (area.x as i32, area.width as i32);

    let margin = MARGIN as i32;
    let (y, height) = (area.y as i32 + margin, area.height as i32 - margin * 2);
    let view = area_width - margin * 2;

    let columns = strip(workspace, area_width);
    let end = columns.last().map_or(0, |&(_, start, width)| start + width);

    // A dialog keeps in view the column that opened it, the closest one before it
    let anchor = workspace
        .focused_client
        .and_then(|w| workspace.position(w))
        .and_then(|idx| {
            workspace.clients[..=idx]
                .iter()
                .rev()
                .find(|c| c.is_tiled())
        })
        .map(|c| c.window);

    let mut scroll = workspace.scroll;
    if let Some(&(_, start, width)) = columns.iter().find(|c| Some(c.0) == anchor) {
        if start < scroll || width >= view {
            scroll = start;
        } else if start + width > scroll + view {
            scroll = start + width - view;
        }
    }
    scroll = scroll.clamp(0, (end - view).max(0));

    let placements = workspace
        .clients
        .iter()
        .filter_map(|client| {
            let placement = if client.fullscreen {
                Placement {
                    decorated: false,
                    ..Placement::framed(mon_x, mon_y, mon_width, mon_height)
                }
            } else if let Some(rect) = client.floating {
                let width = (rect.width as i32).min(mon_width);
                let height = (rect.height as i32).min(mon_height);
                let x = (rect.x as i32).clamp(mon_x, mon_x + mon_width - width);
                let y = (rect.y as i32).clamp(mon_y, mon_y + mon_height - height);
                Placement::framed(x, y, width, height)
            } else {
                let &(_, start, width) = columns.iter().find(|c| c.0 == client.window)?;
                let x = area_x + margin + start - scroll;
                let visible = (x.max(area_x), (x + width).min(area_x + area_width));

                if visible.0 >= visible.1 {
                    // Monitors never have negative coordinates, so this is outside all of them
                    Placement::framed(-2 * mon_width, y, width, height)
                } else {
                    Placement {
                        clip: (visible != (x, x + width)).then_some(visible),
                        ..Placement::framed(x, y, width, height)
                    }
                }
            };
            Some((client.window, client.frame, placement))
        })
        .collect();

    (scroll, placements)
}

impl WindowManager {
    pub fn layout(&mut self) -> Result<()> {
        self.layout_monitor(self.monitors.current_monitor)
    }

    pub fn layout_all_monitors(&mut self) -> Result<()> {
        for monitor_id in 0..self.monitors.count() {
            self.layout_monitor(monitor_id)?;
        }
        Ok(())
    }

    pub fn layout_monitor(&mut self, monitor_id: usize) -> Result<()> {
        let Some(monitor) = self.monitors.get(monitor_id) else {
            return Ok(());
        };

        let (scroll, placements) = arrange(
            monitor.workspaces.current(),
            monitor.geometry,
            monitor.work_area(),
        );

        self.monitors.monitors[monitor_id]
            .workspaces
            .current_mut()
            .scroll = scroll;

        for (window, frame, placement) in placements {
            self.configure_client(window, frame, &placement)?;
            if !placement.decorated {
                self.conn.configure_window(
                    frame,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )?;
            }
        }

        self.raise_floating(monitor_id)?;
        self.restack_alerts()?;
        self.ignore_pending_enters()?;
        self.update_bars()?;
        self.conn.flush()?;
        Ok(())
    }

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

    pub fn resize_column(&mut self, delta: f32) -> Result<()> {
        self.set_column_width(|width| width + delta)
    }

    pub fn cycle_column_width(&mut self) -> Result<()> {
        self.set_column_width(|width| {
            COLUMN_WIDTH_PRESETS
                .into_iter()
                .find(|&preset| preset > width + 0.01)
                .unwrap_or(COLUMN_WIDTH_PRESETS[0])
        })
    }

    pub fn toggle_maximize_column(&mut self) -> Result<()> {
        self.set_column_width(|width| {
            if width < 1.0 {
                1.0
            } else {
                DEFAULT_COLUMN_WIDTH
            }
        })
    }

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

    fn configure_client(
        &mut self,
        window: Window,
        frame: Window,
        placement: &Placement,
    ) -> Result<()> {
        let Placement {
            x,
            y,
            width,
            height,
            decorated,
            clip,
        } = *placement;

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
            client.geometry = Rect {
                x: x as i16,
                y: y as i16,
                width: width as u16,
                height: height as u16,
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect {
        x: 0,
        y: 0,
        width: 800,
        height: 600,
    };

    fn workspace(widths: &[f32], focused: usize) -> Workspace {
        let mut workspace = Workspace::new(0);
        for (i, &width) in widths.iter().enumerate() {
            let mut client = Client::new(i as Window + 1, i as Window + 101, String::new(), None);
            client.column_width = width;
            workspace.clients.push(client);
        }
        workspace.focused_client = Some(focused as Window + 1);
        workspace
    }

    fn placement(placements: &[(Window, Window, Placement)], window: Window) -> &Placement {
        &placements.iter().find(|p| p.0 == window).unwrap().2
    }

    #[test]
    fn strip_splits_the_width_with_margins() {
        let columns = strip(&workspace(&[0.5, 0.5, 1.0], 0), 800);
        assert_eq!(columns, vec![(1, 0, 385), (2, 395, 385), (3, 790, 780)]);
    }

    #[test]
    fn scrolls_just_enough_to_show_the_focused_column() {
        let (scroll, placements) = arrange(&workspace(&[0.5, 0.5, 0.5], 2), MONITOR, MONITOR);

        assert_eq!(scroll, 395);
        assert_eq!(placement(&placements, 1).x, -1600);
        assert_eq!(
            *placement(&placements, 2),
            Placement::framed(10, 10, 385, 580)
        );
        assert_eq!(
            *placement(&placements, 3),
            Placement::framed(405, 10, 385, 580)
        );
    }

    #[test]
    fn clips_a_column_cut_by_the_monitor_edge() {
        let (scroll, placements) = arrange(&workspace(&[0.7, 0.7], 0), MONITOR, MONITOR);

        assert_eq!(scroll, 0);
        assert_eq!(placement(&placements, 2).clip, Some((563, 800)));
        assert_eq!(placement(&placements, 1).clip, None);
    }

    #[test]
    fn never_scrolls_past_the_ends() {
        let mut ws = workspace(&[0.5, 0.5], 0);
        ws.scroll = 5000;
        assert_eq!(arrange(&ws, MONITOR, MONITOR).0, 0);
    }

    #[test]
    fn fullscreen_covers_the_monitor_without_decorations() {
        let mut ws = workspace(&[0.5, 0.5], 1);
        ws.clients[1].fullscreen = true;
        let area = Rect {
            y: 30,
            height: 570,
            ..MONITOR
        };

        let (_, placements) = arrange(&ws, MONITOR, area);

        let fullscreen = placement(&placements, 2);
        assert_eq!(
            (
                fullscreen.x,
                fullscreen.y,
                fullscreen.width,
                fullscreen.height
            ),
            (0, 0, 800, 600)
        );
        assert!(!fullscreen.decorated);
        assert_eq!(placement(&placements, 1).y, 40);
    }

    #[test]
    fn floating_clients_stay_inside_the_monitor() {
        let mut ws = workspace(&[0.5], 0);
        ws.clients[0].floating = Some(Rect {
            x: 700,
            y: -50,
            width: 300,
            height: 200,
        });

        let (_, placements) = arrange(&ws, MONITOR, MONITOR);
        assert_eq!(
            *placement(&placements, 1),
            Placement::framed(500, 0, 300, 200)
        );
    }

    #[test]
    fn a_dialog_keeps_its_column_in_view() {
        let mut ws = workspace(&[0.5, 0.5, 0.5, 0.5], 1);
        ws.clients[1].floating = Some(Rect {
            x: 100,
            y: 100,
            width: 200,
            height: 100,
        });
        ws.scroll = 395;

        assert_eq!(arrange(&ws, MONITOR, MONITOR).0, 0);
    }
}
