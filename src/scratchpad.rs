use anyhow::Result;
use x11rb::protocol::xproto::*;

use crate::clients::{Client, Rect};
use crate::config::{SCRATCHPAD_COMMAND, SCRATCHPAD_SIZE};
use crate::wm::WindowManager;

impl WindowManager {
    pub fn scratchpad_rect(&self) -> Rect {
        let monitor = self.monitors.current().geometry;
        self.centered_rect(
            (monitor.width as f32 * SCRATCHPAD_SIZE.0) as u32,
            (monitor.height as f32 * SCRATCHPAD_SIZE.1) as u32,
        )
    }

    pub fn toggle_scratchpad(&mut self) -> Result<()> {
        if let Some(client) = self.scratchpad.take() {
            return self.show_scratchpad(client);
        }

        let shown = self
            .monitors
            .monitors
            .iter()
            .flat_map(|m| m.workspaces.workspaces.iter())
            .flat_map(|ws| ws.clients.iter())
            .find(|c| c.scratchpad)
            .map(|c| c.window);
        let Some(window) = shown else {
            self.run_command(SCRATCHPAD_COMMAND, false);
            return Ok(());
        };
        let Some((monitor_id, workspace_id)) = self.find_client(window) else {
            return Ok(());
        };

        let focused_here = monitor_id == self.monitors.current_monitor
            && self.is_workspace_visible(monitor_id, workspace_id)
            && self.focused_client() == Some(window);

        let Some(client) = self
            .workspace_of_mut(monitor_id, workspace_id)
            .and_then(|ws| ws.remove_client(window))
        else {
            return Ok(());
        };

        if self.is_workspace_visible(monitor_id, workspace_id) {
            self.layout_monitor(monitor_id)?;
        }

        if focused_here {
            self.conn.unmap_window(client.frame)?;
            self.scratchpad = Some(client);
            self.focus_current()
        } else {
            self.show_scratchpad(client)
        }
    }

    fn show_scratchpad(&mut self, mut client: Client) -> Result<()> {
        client.floating = Some(self.scratchpad_rect());
        let (window, frame) = (client.window, client.frame);

        let workspace = self.workspace_mut();
        workspace.add_client(client);
        workspace.focused_client = Some(window);

        self.conn.map_window(frame)?;
        self.layout()?;
        self.focus_current()
    }
}
