use crate::workspaces::Workspace;
use crate::{wm::WindowManager, workspaces::WorkspaceManager};
use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

#[derive(Debug, Clone)]
pub struct Monitor {
    pub id: usize,
    pub name: String,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub primary: bool,
    pub workspaces: WorkspaceManager,
}

impl Monitor {
    pub fn new(
        id: usize,
        name: String,
        x: i16,
        y: i16,
        width: u16,
        height: u16,
        primary: bool,
        num_workspaces: u8,
    ) -> Self {
        let workspaces = WorkspaceManager::new(num_workspaces);

        Self {
            id,
            name,
            x,
            y,
            width,
            height,
            primary,
            workspaces,
        }
    }

    pub fn current_workspace(&self) -> &Workspace {
        self.workspaces.current()
    }

    pub fn current_workspace_mut(&mut self) -> &mut Workspace {
        self.workspaces.current_mut()
    }

    pub fn switch_workspace(&mut self, index: usize) -> bool {
        self.workspaces.switch_to(index as u8)
    }

    pub fn all_clients(&self) -> Vec<Window> {
        self.workspaces
            .workspaces
            .iter()
            .flat_map(|ws| ws.clients.keys().copied())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct MonitorManager {
    pub monitors: Vec<Monitor>,
    pub current_monitor: usize,
}

#[allow(dead_code)]
impl MonitorManager {
    pub fn detect(conn: &RustConnection, root: Window, num_workspaces: u8) -> Result<Self> {
        let screens_resources = conn.randr_get_screen_resources(root)?.reply().unwrap();

        let mut monitors: Vec<Monitor> = Vec::new();
        let mut monitor_id = 0;

        let primary = conn.randr_get_output_primary(root)?.reply().unwrap().output;

        for &crtc in &screens_resources.crtcs {
            let crtc_info = conn.randr_get_crtc_info(crtc, 0)?.reply().unwrap();

            if crtc_info.outputs.is_empty() || crtc_info.width == 0 || crtc_info.height == 0 {
                continue;
            }

            let output = crtc_info.outputs[0];
            let output_info = conn.randr_get_output_info(output, 0)?.reply().unwrap();

            let name = String::from_utf8_lossy(&output_info.name).to_string();
            let is_primary = output == primary;

            monitors.push(Monitor::new(
                monitor_id,
                name,
                crtc_info.x,
                crtc_info.y,
                crtc_info.width,
                crtc_info.height,
                is_primary,
                num_workspaces,
            ));

            monitor_id += 1;
        }

        if monitors.is_empty() {
            let geometry = conn.get_geometry(root)?.reply().unwrap();

            monitors.push(Monitor::new(
                0,
                "Default".to_string(),
                0,
                0,
                geometry.width,
                geometry.height,
                true,
                num_workspaces,
            ));
        }

        monitors.sort_by_key(|m| (!m.primary, m.x));

        for (i, monitor) in monitors.iter_mut().enumerate() {
            monitor.id = i;
        }

        Ok(Self {
            monitors,
            current_monitor: 0,
        })
    }

    pub fn refresh(&mut self, conn: &RustConnection, root: Window) -> Result<Vec<MonitorChange>> {
        let old_monitors = self.monitors.clone();
        let num_workspaces = old_monitors
            .first()
            .map(|m| m.workspaces.workspaces.len() as u8)
            .unwrap_or(9);

        let new_manager = Self::detect(&conn, root, num_workspaces)?;
        let mut changes = Vec::new();

        for old_monitor in &old_monitors {
            if !new_manager
                .monitors
                .iter()
                .any(|m| m.name == old_monitor.name)
            {
                changes.push(MonitorChange::Removed(old_monitor.id));
            }
        }

        for new_monitor in &new_manager.monitors {
            if !old_monitors.iter().any(|m| m.name == new_monitor.name) {
                changes.push(MonitorChange::Added(new_monitor.id));
            }
        }

        let removed_ids: Vec<_> = changes
            .iter()
            .filter_map(|c| {
                if let MonitorChange::Removed(id) = c {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        if let Some(primary_idx) = new_manager.monitors.iter().position(|m| m.primary) {
            for old_id in removed_ids {
                if old_monitors.get(old_id).is_some() {
                    println!(
                        "Migrating windows from removed monitor {} to primary monitor",
                        old_id
                    );

                    changes.push(MonitorChange::MigrateWindows {
                        from: old_id,
                        to: primary_idx,
                    });
                }
            }
        }

        self.monitors = new_manager.monitors;
        self.current_monitor = self
            .current_monitor
            .min(self.monitors.len().saturating_sub(1));

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

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Monitor> {
        self.monitors.get_mut(id)
    }

    pub fn switch_to(&mut self, id: usize) -> bool {
        if id < self.monitors.len() {
            self.current_monitor = id;
            true
        } else {
            false
        }
    }

    // pub fn find_monitor_for_window(&self, x: i16, y: i16, width: u16, height: u16) -> usize {
    //     for monitor in &self.monitors {
    //         if monitor.contains_window(x, y, width, height) {
    //             return monitor.id;
    //         }
    //     }
    //     self.current_monitor
    // }

    pub fn count(&self) -> usize {
        self.monitors.len()
    }

    pub fn next_monitor_id(&self) -> usize {
        (self.current_monitor + 1) % self.monitors.len()
    }

    pub fn prev_monitor_id(&self) -> usize {
        if self.current_monitor == 0 {
            self.monitors.len() - 1
        } else {
            self.current_monitor - 1
        }
    }
}

#[allow(dead_code)]
impl WindowManager {
    pub fn handle_monitor_changes(&mut self, changes: Vec<MonitorChange>) -> Result<()> {
        for change in changes {
            match change {
                MonitorChange::Added(id) => {
                    println!("Monitor {} added", id);
                    self.draw_alert(format!("Monitor added: {}", id))?;
                }
                MonitorChange::Removed(id) => {
                    println!("Monitor {} removed", id);
                    self.draw_alert(format!("Monitor removed: {}", id))?;
                }
                MonitorChange::MigrateWindows { from, to } => {
                    println!("Migrating windows from monitor {} to {}", from, to);
                }
            }
        }

        self.layout_all_monitors()?;

        Ok(())
    }

    pub fn layout_all_monitors(&mut self) -> Result<()> {
        let num_monitors = self.monitors.count();

        for monitor_id in 0..num_monitors {
            let saved_monitor = self.monitors.current_monitor;

            self.monitors.switch_to(monitor_id);

            self.layout()?;

            self.monitors.switch_to(saved_monitor);
        }

        Ok(())
    }

    /// Change focus to next monitor
    pub fn focus_next_monitor(&mut self) -> Result<()> {
        let next = self.monitors.next_monitor_id();
        self.focus_monitor(next)
    }

    /// Change focus to prev monitor
    pub fn focus_prev_monitor(&mut self) -> Result<()> {
        let prev = self.monitors.prev_monitor_id();
        self.focus_monitor(prev)
    }

    /// Focus a specific monitor
    pub fn focus_monitor(&mut self, monitor_id: usize) -> Result<()> {
        if monitor_id >= self.monitors.count() {
            return Ok(());
        }

        if self.monitors.current_monitor == monitor_id {
            return Ok(());
        }

        println!("Focusing monitor {}", monitor_id);

        self.monitors.switch_to(monitor_id);

        let monitor = self.monitors.current();

        let x = monitor.width.saturating_sub(10) / 2;
        let y = monitor.height.saturating_sub(10) / 2;

        self.conn.warp_pointer(
            x11rb::NONE,
            self.root,
            0,
            0,
            0,
            0,
            monitor.x + x as i16,
            monitor.y + y as i16,
        )?;

        // Focus current window on active workspace
        if let Some(focused) = self.focused_client() {
            self.conn
                .set_input_focus(InputFocus::PARENT, focused, CURRENT_TIME)?;
            self.update_client_borders()?;
        }

        let monitor = self.monitors.current();
        self.draw_alert(format!("Monitor: {} ({})", monitor.id, monitor.name))?;

        self.conn.flush()?;
        Ok(())
    }

    /// Move focused window to next monitor
    pub fn move_focused_to_next_monitor(&mut self) -> Result<()> {
        let next = self.monitors.next_monitor_id();
        self.move_focused_to_monitor(next)
    }

    /// Move focused window to prev monitor
    pub fn move_focused_to_prev_monitor(&mut self) -> Result<()> {
        let prev = self.monitors.prev_monitor_id();
        self.move_focused_to_monitor(prev)
    }

    /// Move focused window to a specific monitor
    pub fn move_focused_to_monitor(&mut self, target_monitor_id: usize) -> Result<()> {
        if target_monitor_id >= self.monitors.count() {
            return Ok(());
        }

        let current_monitor_id = self.monitors.current_monitor;

        if current_monitor_id == target_monitor_id {
            return Ok(());
        }

        if let Some(window) = self.focused_client() {
            println!(
                "Moving window {} from monitor {} to monitor {}",
                window, current_monitor_id, target_monitor_id
            );

            let current_ws_index = self.monitors.current().current_workspace();

            let state = self
                .monitors
                .current_mut()
                .current_workspace_mut()
                .remove_client(window)
                .ok_or_else(|| anyhow::anyhow!("Window not found"))?;

            let target_ws_id = {
                let target_monitor = self
                    .monitors
                    .monitors
                    .get(target_monitor_id)
                    .ok_or_else(|| anyhow::anyhow!("Invalid target monitor"))?;

                target_monitor.current_workspace().id
            };

            if let Some(ws) = self.monitors.monitors[target_monitor_id]
                .workspaces
                .get_mut(target_ws_id)
            {
                ws.add_client(window, state);
            }

            let _saved_monitor = self.monitors.current_monitor;
            self.monitors.switch_to(current_monitor_id);
            self.layout()?;

            self.monitors.switch_to(target_monitor_id);
            self.layout()?;

            self.conn
                .set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)?;
            self.update_client_borders()?;

            let target_monitor = self.monitors.current();
            self.draw_alert(format!(
                "Monitor: {} ({})",
                target_monitor.id, target_monitor.name
            ))?;

            self.conn.flush()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum MonitorChange {
    Added(usize),
    Removed(usize),
    MigrateWindows { from: usize, to: usize },
}
