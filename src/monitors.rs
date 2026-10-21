use std::collections::HashSet;

use anyhow::Result;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::clients::Rect;
use crate::wm::WindowManager;
use crate::workspaces::WorkspaceManager;

#[derive(Debug, Clone)]
struct OutputGeometry {
    name: String,
    geometry: Rect,
    primary: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reserved {
    pub left: u16,
    pub right: u16,
    pub top: u16,
    pub bottom: u16,
}

#[derive(Debug, Clone)]
pub struct Monitor {
    pub name: String,
    pub geometry: Rect,
    pub primary: bool,
    pub reserved: Reserved,
    pub workspaces: WorkspaceManager,
}

impl Monitor {
    fn new(output: OutputGeometry, num_workspaces: usize) -> Self {
        Self {
            name: output.name,
            geometry: output.geometry,
            primary: output.primary,
            reserved: Reserved::default(),
            workspaces: WorkspaceManager::new(num_workspaces),
        }
    }

    pub fn work_area(&self) -> Rect {
        let (g, r) = (self.geometry, self.reserved);
        let width = g.width.saturating_sub(r.left + r.right).max(1);
        let height = g.height.saturating_sub(r.top + r.bottom).max(1);

        Rect {
            x: g.x + r.left.min(g.width - width) as i16,
            y: g.y + r.top.min(g.height - height) as i16,
            width,
            height,
        }
    }
}

#[derive(Debug, Default)]
pub struct MonitorChanges {
    pub added: Vec<String>,
    pub removed: Vec<Monitor>,
    pub resized: bool,
}

impl MonitorChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && !self.resized
    }
}

#[derive(Debug, Clone)]
pub struct MonitorManager {
    pub monitors: Vec<Monitor>,
    pub current_monitor: usize,
    num_workspaces: usize,
}

impl MonitorManager {
    pub fn detect(conn: &RustConnection, root: Window, num_workspaces: usize) -> Result<Self> {
        let monitors = Self::query_outputs(conn, root)?
            .into_iter()
            .map(|output| Monitor::new(output, num_workspaces))
            .collect();

        let mut manager = Self {
            monitors,
            current_monitor: 0,
            num_workspaces,
        };

        let pointer = conn.query_pointer(root)?.reply()?;
        manager.current_monitor = manager
            .monitor_at(pointer.root_x, pointer.root_y)
            .unwrap_or_else(|| manager.primary_index());

        Ok(manager)
    }

    fn query_outputs(conn: &RustConnection, root: Window) -> Result<Vec<OutputGeometry>> {
        let mut outputs = Vec::new();

        if conn
            .extension_information(randr::X11_EXTENSION_NAME)?
            .is_some()
        {
            let version = conn.randr_query_version(1, 5)?.reply()?;

            if (version.major_version, version.minor_version) >= (1, 5) {
                for monitor in conn.randr_get_monitors(root, true)?.reply()?.monitors {
                    if monitor.width == 0 || monitor.height == 0 {
                        continue;
                    }

                    let name = conn.get_atom_name(monitor.name)?.reply()?.name;

                    outputs.push(OutputGeometry {
                        name: String::from_utf8_lossy(&name).to_string(),
                        geometry: Rect {
                            x: monitor.x,
                            y: monitor.y,
                            width: monitor.width,
                            height: monitor.height,
                        },
                        primary: monitor.primary,
                    });
                }
            }
        }

        // Mirrors share the same area
        let mut seen = HashSet::new();
        outputs.retain(|o| {
            let g = o.geometry;
            seen.insert((g.x, g.y, g.width, g.height))
        });

        // An output covering others is a mirror with a bigger resolution or an area
        // split with `xrandr --setmonitor`, keep the smaller ones
        let covers = |a: &OutputGeometry, b: &OutputGeometry| {
            let (a, b) = (a.geometry, b.geometry);
            a.x <= b.x
                && a.y <= b.y
                && a.x as i32 + a.width as i32 >= b.x as i32 + b.width as i32
                && a.y as i32 + a.height as i32 >= b.y as i32 + b.height as i32
        };
        let snapshot = outputs.clone();
        outputs.retain(|a| !snapshot.iter().any(|b| a.name != b.name && covers(a, b)));

        if outputs.is_empty() {
            let geometry = conn.get_geometry(root)?.reply()?;

            outputs.push(OutputGeometry {
                name: "default".to_string(),
                geometry: Rect {
                    x: 0,
                    y: 0,
                    width: geometry.width,
                    height: geometry.height,
                },
                primary: true,
            });
        }

        outputs.sort_by_key(|o| (o.geometry.x, o.geometry.y));

        Ok(outputs)
    }

    pub fn refresh(&mut self, conn: &RustConnection, root: Window) -> Result<MonitorChanges> {
        let outputs = Self::query_outputs(conn, root)?;
        let current_name = self.current().name.clone();

        let mut old_monitors = std::mem::take(&mut self.monitors);
        let mut changes = MonitorChanges::default();

        for output in outputs {
            let monitor = match old_monitors.iter().position(|m| m.name == output.name) {
                Some(pos) => {
                    let mut monitor = old_monitors.remove(pos);

                    if monitor.geometry != output.geometry {
                        changes.resized = true;
                    }

                    monitor.geometry = output.geometry;
                    monitor.primary = output.primary;
                    monitor
                }
                None => {
                    changes.added.push(output.name.clone());
                    Monitor::new(output, self.num_workspaces)
                }
            };

            self.monitors.push(monitor);
        }

        changes.removed = old_monitors;

        self.current_monitor = self
            .monitors
            .iter()
            .position(|m| m.name == current_name)
            .unwrap_or_else(|| self.primary_index());

        Ok(changes)
    }

    pub fn current(&self) -> &Monitor {
        &self.monitors[self.current_monitor]
    }

    pub fn current_mut(&mut self) -> &mut Monitor {
        &mut self.monitors[self.current_monitor]
    }

    pub fn get(&self, id: usize) -> Option<&Monitor> {
        self.monitors.get(id)
    }

    pub fn count(&self) -> usize {
        self.monitors.len()
    }

    pub fn monitor_at(&self, x: i16, y: i16) -> Option<usize> {
        self.monitors.iter().position(|m| m.geometry.contains(x, y))
    }

    pub fn primary_index(&self) -> usize {
        self.monitors.iter().position(|m| m.primary).unwrap_or(0)
    }

    pub fn relative(&self, step: isize) -> usize {
        (self.current_monitor as isize + step).rem_euclid(self.count() as isize) as usize
    }
}

impl WindowManager {
    pub fn refresh_monitors(&mut self) -> Result<()> {
        let changes = self.monitors.refresh(&self.conn, self.root)?;

        if changes.is_empty() {
            return Ok(());
        }

        let target = self.monitors.primary_index();
        let mut messages = Vec::new();

        for monitor in changes.removed {
            println!(
                "Monitor {} removed, migrating its clients to {}",
                monitor.name, self.monitors.monitors[target].name
            );
            messages.push(format!("-{}", monitor.name));
            self.migrate_monitor_clients(monitor, target)?;
        }

        for name in changes.added {
            println!("Monitor {} added", name);
            messages.push(format!("+{}", name));
        }

        self.update_struts()?;
        self.focus_current()?;

        if !messages.is_empty() {
            self.draw_alert(format!("[MON] {}", messages.join(" ")))?;
        }

        Ok(())
    }

    fn migrate_monitor_clients(&mut self, monitor: Monitor, target: usize) -> Result<()> {
        let was_visible_id = monitor.workspaces.current_workspace;
        let target_workspaces = &self.monitors.monitors[target].workspaces;
        let is_visible_id = target_workspaces.current_workspace;

        for workspace in monitor.workspaces.workspaces {
            let was_visible = workspace.id == was_visible_id;
            let is_visible = workspace.id == is_visible_id;
            let windows = workspace.windows();

            let Some(destination) = self.monitors.monitors[target]
                .workspaces
                .get_mut(workspace.id)
            else {
                continue;
            };

            if destination.focused_client.is_none() {
                destination.focused_client = workspace.focused_client;
            }
            destination.clients.extend(workspace.clients);

            for window in windows {
                match (was_visible, is_visible) {
                    (true, false) => self.hide_client(window)?,
                    (false, true) => self.show_client(window)?,
                    _ => {}
                }
            }
        }

        Ok(())
    }

    pub fn focus_next_monitor(&mut self) -> Result<()> {
        self.focus_monitor(self.monitors.relative(1), true)
    }

    pub fn focus_prev_monitor(&mut self) -> Result<()> {
        self.focus_monitor(self.monitors.relative(-1), true)
    }

    pub fn focus_monitor_at(&mut self, x: i16, y: i16) -> Result<()> {
        match self.monitors.monitor_at(x, y) {
            Some(monitor_id) => self.focus_monitor(monitor_id, false),
            None => Ok(()),
        }
    }

    pub fn focus_monitor(&mut self, monitor_id: usize, warp_cursor: bool) -> Result<()> {
        if monitor_id >= self.monitors.count() || self.monitors.current_monitor == monitor_id {
            return Ok(());
        }

        println!("Focusing monitor {}", monitor_id);

        self.monitors.current_monitor = monitor_id;

        if warp_cursor {
            self.warp_pointer_to_focus()?;
        }

        self.focus_current()?;

        let name = self.monitors.current().name.clone();
        self.draw_alert(format!("[MON] {} ({})", monitor_id, name))
    }

    pub fn warp_pointer_to_focus(&mut self) -> Result<()> {
        let (x, y) = match self.focused_client().and_then(|w| self.client(w)) {
            Some(client) => client.geometry.center(),
            None => self.monitors.current().geometry.center(),
        };

        self.conn
            .warp_pointer(x11rb::NONE, self.root, 0, 0, 0, 0, x, y)?;

        self.ignore_pending_enters()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn banish_pointer(&mut self) -> Result<()> {
        let g = self.monitors.current().geometry;
        let x = g.x + g.width as i16 - 1;
        let y = g.y + g.height as i16 - 1;

        self.conn
            .warp_pointer(x11rb::NONE, self.root, 0, 0, 0, 0, x, y)?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn move_focused_to_next_monitor(&mut self) -> Result<()> {
        self.move_focused_to_monitor(self.monitors.relative(1))
    }

    pub fn move_focused_to_prev_monitor(&mut self) -> Result<()> {
        self.move_focused_to_monitor(self.monitors.relative(-1))
    }

    pub fn move_focused_to_monitor(&mut self, target_id: usize) -> Result<()> {
        let source_id = self.monitors.current_monitor;

        if target_id >= self.monitors.count() || source_id == target_id {
            return Ok(());
        }

        let Some(window) = self.focused_client() else {
            return Ok(());
        };

        let Some(mut client) = self.workspace_mut().remove_client(window) else {
            return Ok(());
        };

        if let Some(rect) = &mut client.floating {
            let (source, target) = (
                self.monitors.monitors[source_id].geometry,
                self.monitors.monitors[target_id].geometry,
            );
            rect.x += target.x - source.x;
            rect.y += target.y - source.y;
        }

        println!(
            "Moving client {} from monitor {} to monitor {}",
            window, source_id, target_id
        );

        let target = self.monitors.monitors[target_id].workspaces.current_mut();
        target.add_client(client);
        target.focused_client = Some(window);

        self.monitors.current_monitor = target_id;

        self.layout_monitor(source_id)?;
        self.layout_monitor(target_id)?;

        self.warp_pointer_to_focus()?;
        self.focus_current()?;

        let name = self.monitors.current().name.clone();
        self.draw_alert(format!("[MVMON] {} ({})", target_id, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(reserved: Reserved) -> Monitor {
        Monitor {
            name: "test".to_string(),
            geometry: Rect {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            primary: false,
            reserved,
            workspaces: WorkspaceManager::new(1),
        }
    }

    #[test]
    fn work_area_subtracts_the_reserved_edges() {
        let area = monitor(Reserved {
            left: 10,
            top: 30,
            ..Reserved::default()
        })
        .work_area();

        assert_eq!(
            area,
            Rect {
                x: 1930,
                y: 30,
                width: 1910,
                height: 1050
            }
        );
    }

    #[test]
    fn work_area_is_never_empty() {
        let area = monitor(Reserved {
            left: 2000,
            ..Reserved::default()
        })
        .work_area();

        assert_eq!((area.x, area.width), (3839, 1));
    }
}
