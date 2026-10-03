use anyhow::Result;
use x11rb::protocol::xproto::Window;

use crate::clients::Client;
use crate::wm::WindowManager;

#[derive(Debug, Clone)]
pub struct Workspace {
    /// 1-based, matches the number key used to reach it
    pub id: u8,
    /// Clients in layout order
    pub clients: Vec<Client>,
    pub focused_client: Option<Window>,
}

impl Workspace {
    pub fn new(id: u8) -> Self {
        Self {
            id,
            clients: Vec::new(),
            focused_client: None,
        }
    }

    pub fn position(&self, window: Window) -> Option<usize> {
        self.clients.iter().position(|c| c.window == window)
    }

    pub fn get(&self, window: Window) -> Option<&Client> {
        self.clients.iter().find(|c| c.window == window)
    }

    pub fn get_mut(&mut self, window: Window) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.window == window)
    }

    pub fn windows(&self) -> Vec<Window> {
        self.clients.iter().map(|c| c.window).collect()
    }

    pub fn add_client(&mut self, client: Client) {
        if self.focused_client.is_none() {
            self.focused_client = Some(client.window);
        }
        self.clients.push(client);
    }

    /// Remove a client. If it was focused, the focus moves to its right neighbour
    /// (or the left one if it was the last)
    pub fn remove_client(&mut self, window: Window) -> Option<Client> {
        let idx = self.position(window)?;
        let client = self.clients.remove(idx);

        if self.focused_client == Some(window) {
            self.focused_client = self
                .clients
                .get(idx)
                .or_else(|| self.clients.last())
                .map(|c| c.window);
        }

        Some(client)
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub current_workspace: u8,
    pub last_workspace: u8,
}

impl WorkspaceManager {
    pub fn new(num_workspaces: u8) -> Self {
        Self {
            workspaces: (1..=num_workspaces).map(Workspace::new).collect(),
            current_workspace: 1,
            last_workspace: 2,
        }
    }

    pub fn current(&self) -> &Workspace {
        &self.workspaces[(self.current_workspace - 1) as usize]
    }

    pub fn current_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[(self.current_workspace - 1) as usize]
    }

    pub fn get_mut(&mut self, id: u8) -> Option<&mut Workspace> {
        self.workspaces.get_mut((id as usize).checked_sub(1)?)
    }

    pub fn contains(&self, id: u8) -> bool {
        (1..=self.workspaces.len()).contains(&(id as usize))
    }
}

impl WindowManager {
    /// Show another workspace on the current monitor. Returns whether it changed
    pub fn switch_to_workspace(&mut self, workspace_id: u8) -> Result<bool> {
        let workspaces = &self.monitors.current().workspaces;
        if !workspaces.contains(workspace_id) || workspaces.current_workspace == workspace_id {
            return Ok(false);
        }

        println!("Switching to workspace {}", workspace_id);

        for window in self.workspace().windows() {
            self.hide_client(window)?;
        }

        let workspaces = &mut self.monitors.current_mut().workspaces;
        workspaces.last_workspace = workspaces.current_workspace;
        workspaces.current_workspace = workspace_id;

        self.layout()?;

        for window in self.workspace().windows() {
            self.show_client(window)?;
        }

        self.focus_current()?;
        Ok(true)
    }

    /// Send the focused client to another workspace of the current monitor
    pub fn move_focused_to_workspace(&mut self, workspace_id: u8) -> Result<bool> {
        let workspaces = &self.monitors.current().workspaces;
        if !workspaces.contains(workspace_id) || workspaces.current_workspace == workspace_id {
            return Ok(false);
        }

        let Some(window) = self.focused_client() else {
            return Ok(false);
        };

        println!("Moving client {} to workspace {}", window, workspace_id);

        self.hide_client(window)?;

        let workspaces = &mut self.monitors.current_mut().workspaces;
        if let Some(client) = workspaces.current_mut().remove_client(window)
            && let Some(target) = workspaces.get_mut(workspace_id)
        {
            target.add_client(client);
        }

        self.layout()?;
        self.focus_current()?;
        Ok(true)
    }

    pub fn cycle_last_workspace(&mut self) -> Result<()> {
        let last = self.monitors.current().workspaces.last_workspace;
        self.switch_to_workspace(last)?;
        Ok(())
    }
}
