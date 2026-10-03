use std::collections::HashSet;

use anyhow::Result;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::WindowManager;
use crate::workspaces::WorkspaceManager;

/// Area of the root window reported by RandR
#[derive(Debug, Clone)]
struct OutputGeometry {
    name: String,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    primary: bool,
}

/// A screen area with its own independent set of workspaces
#[derive(Debug, Clone)]
pub struct Monitor {
    pub name: String,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub primary: bool,
    pub workspaces: WorkspaceManager,
}

impl Monitor {
    fn new(output: OutputGeometry, num_workspaces: u8) -> Self {
        Self {
            name: output.name,
            x: output.x,
            y: output.y,
            width: output.width,
            height: output.height,
            primary: output.primary,
            workspaces: WorkspaceManager::new(num_workspaces),
        }
    }

    /// Check if a point (root coordinates) is inside the monitor
    pub fn contains(&self, x: i16, y: i16) -> bool {
        let (x, y) = (x as i32, y as i32);
        x >= self.x as i32
            && x < self.x as i32 + self.width as i32
            && y >= self.y as i32
            && y < self.y as i32 + self.height as i32
    }
}

/// Result of reconciling the monitor list with the X server
#[derive(Debug, Default)]
pub struct MonitorChanges {
    pub added: Vec<String>,
    /// Removed monitors, still holding their workspaces (and clients) to be migrated
    pub removed: Vec<Monitor>,
    pub resized: bool,
}

impl MonitorChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && !self.resized
    }
}

/// Monitors sorted left to right. Outputs are enabled by the user (e.g. with
/// xrandr), dxwm only follows what RandR reports
#[derive(Debug, Clone)]
pub struct MonitorManager {
    pub monitors: Vec<Monitor>,
    pub current_monitor: usize,
    num_workspaces: u8,
}

impl MonitorManager {
    pub fn detect(conn: &RustConnection, root: Window, num_workspaces: u8) -> Result<Self> {
        let monitors = Self::query_outputs(conn, root)?
            .into_iter()
            .map(|output| Monitor::new(output, num_workspaces))
            .collect();

        let mut manager = Self {
            monitors,
            current_monitor: 0,
            num_workspaces,
        };

        // Start on the monitor under the pointer, falling back to the primary one
        let pointer = conn.query_pointer(root)?.reply()?;
        manager.current_monitor = manager
            .monitor_at(pointer.root_x, pointer.root_y)
            .unwrap_or_else(|| manager.primary_index());

        Ok(manager)
    }

    /// Active RandR monitors (including `xrandr --setmonitor` ones), or the whole
    /// root window when RandR 1.5 is not available
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
                        x: monitor.x,
                        y: monitor.y,
                        width: monitor.width,
                        height: monitor.height,
                        primary: monitor.primary,
                    });
                }
            }
        }

        // Mirrored outputs share the same area, keep only one of them
        let mut seen = HashSet::new();
        outputs.retain(|o| seen.insert((o.x, o.y, o.width, o.height)));

        // An output covering others is a mirror with a bigger resolution or an area
        // split with `xrandr --setmonitor`, keep the smaller ones
        let covers = |a: &OutputGeometry, b: &OutputGeometry| {
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
                x: 0,
                y: 0,
                width: geometry.width,
                height: geometry.height,
                primary: true,
            });
        }

        outputs.sort_by_key(|o| (o.x, o.y));

        Ok(outputs)
    }

    /// Re-read the outputs keeping the workspaces of the monitors that still exist
    /// (matched by name). Removed monitors are returned so their clients can be migrated
    pub fn refresh(&mut self, conn: &RustConnection, root: Window) -> Result<MonitorChanges> {
        let outputs = Self::query_outputs(conn, root)?;
        let current_name = self.current().name.clone();

        let mut old_monitors = std::mem::take(&mut self.monitors);
        let mut changes = MonitorChanges::default();

        for output in outputs {
            let monitor = match old_monitors.iter().position(|m| m.name == output.name) {
                Some(pos) => {
                    let mut monitor = old_monitors.remove(pos);

                    if (monitor.x, monitor.y, monitor.width, monitor.height)
                        != (output.x, output.y, output.width, output.height)
                    {
                        changes.resized = true;
                    }

                    monitor.x = output.x;
                    monitor.y = output.y;
                    monitor.width = output.width;
                    monitor.height = output.height;
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

    /// Return the monitor that contains a point (root coordinates)
    pub fn monitor_at(&self, x: i16, y: i16) -> Option<usize> {
        self.monitors.iter().position(|m| m.contains(x, y))
    }

    pub fn primary_index(&self) -> usize {
        self.monitors.iter().position(|m| m.primary).unwrap_or(0)
    }

    /// Index of the monitor `step` positions away from the current one, wrapping around
    pub fn relative(&self, step: isize) -> usize {
        (self.current_monitor as isize + step).rem_euclid(self.count() as isize) as usize
    }
}

impl WindowManager {
    /// Re-read the monitor layout from the X server and apply the changes
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

        self.layout_all_monitors()?;
        self.focus_current()?;

        if !messages.is_empty() {
            self.draw_alert(format!("[MON] {}", messages.join(" ")))?;
        }

        Ok(())
    }

    /// Move every client of a removed monitor to the same workspace of `target`
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
                    (false, true) => {
                        self.conn.map_window(window)?;
                    }
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

    /// Focus the monitor under a point (root coordinates), used by the mouse
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

    /// Put the pointer on the focused client, or the center of the current monitor
    pub fn warp_pointer_to_focus(&mut self) -> Result<()> {
        let monitor = self.monitors.current();

        let (x, y, width, height) = match self.focused_client().and_then(|w| self.client(w)) {
            Some(client) => (client.x, client.y, client.width, client.height),
            None => (monitor.x, monitor.y, monitor.width, monitor.height),
        };

        self.conn.warp_pointer(
            x11rb::NONE,
            self.root,
            0,
            0,
            0,
            0,
            x.saturating_add((width / 2) as i16),
            y.saturating_add((height / 2) as i16),
        )?;

        // The pointer now lies on a different window, that EnterNotify is ours
        self.ignore_pending_enters()?;
        self.conn.flush()?;
        Ok(())
    }

    /// Move the pointer to the bottom right corner of the current monitor
    pub fn banish_pointer(&mut self) -> Result<()> {
        let monitor = self.monitors.current();

        // Last pixel of the monitor, one more would land on the next one
        let x = monitor.x + monitor.width as i16 - 1;
        let y = monitor.y + monitor.height as i16 - 1;

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

    /// Move the focused client to the visible workspace of another monitor (focus follows it)
    pub fn move_focused_to_monitor(&mut self, target_id: usize) -> Result<()> {
        let source_id = self.monitors.current_monitor;

        if target_id >= self.monitors.count() || source_id == target_id {
            return Ok(());
        }

        let Some(window) = self.focused_client() else {
            return Ok(());
        };

        let Some(client) = self.workspace_mut().remove_client(window) else {
            return Ok(());
        };

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
