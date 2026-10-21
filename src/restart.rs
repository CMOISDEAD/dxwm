use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use crate::clients::{Client, Rect};
use crate::config::MIN_COLUMN_WIDTH;
use crate::wm::{Exit, WindowManager};

/// Workspaces are saved by their number (`Workspace::number`)
/// _DXWM_CLIENT: monitor, workspace, position, column width (per mille), focused,
/// floating, x, y, width, height
const CLIENT_STATE_LEN: usize = 10;

impl WindowManager {
    /// The clients go back to the root when the connection closes (they are in the
    /// save-set), and the new process adopts them using the state left on them
    pub fn restart(&mut self) -> Result<()> {
        self.close_prompt()?;
        self.save_state()?;
        self.exit = Some(Exit::Restart);
        Ok(())
    }

    fn save_state(&self) -> Result<()> {
        let monitors = &self.monitors.monitors;

        let current: Vec<u32> = std::iter::once(self.monitors.current_monitor as u32)
            .chain(
                monitors
                    .iter()
                    .map(|m| m.workspaces.current().number() as u32),
            )
            .collect();
        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            self.atoms.dxwm_monitors,
            AtomEnum::CARDINAL,
            &current,
        )?;

        for (monitor_id, monitor) in monitors.iter().enumerate() {
            for workspace in &monitor.workspaces.workspaces {
                for (position, client) in workspace.clients.iter().enumerate() {
                    let rect = client.floating.unwrap_or_default();
                    let state: [u32; CLIENT_STATE_LEN] = [
                        monitor_id as u32,
                        workspace.number() as u32,
                        position as u32,
                        (client.column_width * 1000.0).round() as u32,
                        (workspace.focused_client == Some(client.window)) as u32,
                        client.floating.is_some() as u32,
                        rect.x as i32 as u32,
                        rect.y as i32 as u32,
                        rect.width as u32,
                        rect.height as u32,
                    ];

                    self.conn.change_property32(
                        PropMode::REPLACE,
                        client.window,
                        self.atoms.dxwm_client,
                        AtomEnum::CARDINAL,
                        &state,
                    )?;
                }
            }
        }

        self.conn.flush()?;
        Ok(())
    }

    fn take_property(&self, window: Window, property: Atom) -> Result<Vec<u32>> {
        let reply = self
            .conn
            .get_property(true, window, property, AtomEnum::CARDINAL, 0, 64)?
            .reply()?;
        Ok(reply.value32().map(Iterator::collect).unwrap_or_default())
    }

    pub fn adopt_windows(&mut self) -> Result<()> {
        let children = self.conn.query_tree(self.root)?.reply()?.children;
        let mut adopted: Vec<(Option<Vec<u32>>, Client)> = Vec::new();

        for window in children {
            let Ok(attributes) = self.conn.get_window_attributes(window)?.reply() else {
                continue;
            };
            if attributes.override_redirect
                || attributes.map_state != MapState::VIEWABLE
                || self.is_own_window(window)
            {
                continue;
            }

            if self.is_dock(window)? {
                self.manage_dock(window)?;
                continue;
            }

            let state = self.take_property(window, self.atoms.dxwm_client)?;
            let state = (state.len() == CLIENT_STATE_LEN).then_some(state);
            let fullscreen = self.wants_fullscreen(window)?;

            let rule = self.rule_for(window)?;
            let mut client = self.new_client(window, rule)?;
            client.fullscreen = fullscreen;
            adopted.push((state, client));
        }

        let saved = self.take_property(self.root, self.atoms.dxwm_monitors)?;
        if saved.len() == self.monitors.count() + 1 {
            for (monitor, &number) in self.monitors.monitors.iter_mut().zip(&saved[1..]) {
                if let Some(id) = (number as usize).checked_sub(1)
                    && monitor.workspaces.contains(id)
                {
                    monitor.workspaces.current_workspace = id;
                }
            }
            if (saved[0] as usize) < self.monitors.count() {
                self.monitors.current_monitor = saved[0] as usize;
            }
        }

        adopted.sort_by_key(|(state, _)| state.as_ref().map(|s| (s[0], s[1], s[2])));

        let fallback = (self.monitors.current_monitor, self.workspace().id);

        for (state, mut client) in adopted {
            let (mut monitor_id, mut workspace_id, mut focused) = (fallback.0, fallback.1, false);

            if let Some(s) = &state {
                if let Some(monitor) = self.monitors.get(s[0] as usize)
                    && let Some(id) = (s[1] as usize).checked_sub(1)
                    && monitor.workspaces.contains(id)
                {
                    (monitor_id, workspace_id) = (s[0] as usize, id);
                }
                focused = s[4] == 1;
                client.column_width = (s[3] as f32 / 1000.0).clamp(MIN_COLUMN_WIDTH, 1.0);
                client.floating = (s[5] == 1).then_some(Rect {
                    x: s[6] as i32 as i16,
                    y: s[7] as i32 as i16,
                    width: s[8] as u16,
                    height: s[9] as u16,
                });
            }

            let window = client.window;
            let Some(workspace) = self.monitors.monitors[monitor_id]
                .workspaces
                .get_mut(workspace_id)
            else {
                continue;
            };

            workspace.clients.push(client);
            if focused || workspace.focused_client.is_none() {
                workspace.focused_client = Some(window);
            }
        }

        let visible: Vec<Window> = self
            .monitors
            .monitors
            .iter()
            .flat_map(|m| m.workspaces.current().windows())
            .collect();
        for window in visible {
            self.show_client(window)?;
        }

        self.place_bars()?;
        self.layout_all_monitors()?;
        self.focus_current()
    }
}
