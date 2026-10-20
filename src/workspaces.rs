use anyhow::Result;
use x11rb::protocol::xproto::Window;

use crate::clients::Client;
use crate::wm::WindowManager;

#[derive(Debug, Clone)]
pub struct Workspace {
    /// Index in its monitor, shown to the user as `number()`
    pub id: usize,
    pub clients: Vec<Client>,
    pub focused_client: Option<Window>,
    pub scroll: i32,
}

impl Workspace {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            clients: Vec::new(),
            focused_client: None,
            scroll: 0,
        }
    }

    pub fn number(&self) -> usize {
        self.id + 1
    }

    pub fn position(&self, window: Window) -> Option<usize> {
        self.clients.iter().position(|c| c.window == window)
    }

    /// Positions of the focused client and of the one `step` away from it, clamped
    /// to the ends. None if there is nowhere to go
    pub fn step_from_focused(&self, step: isize) -> Option<(usize, usize)> {
        let current = self.focused_client.and_then(|w| self.position(w))?;
        let target = current
            .saturating_add_signed(step)
            .min(self.clients.len() - 1);
        (target != current).then_some((current, target))
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
        let idx = match self.focused_client.and_then(|w| self.position(w)) {
            Some(focused) => focused + 1,
            None => self.clients.len(),
        };

        if self.focused_client.is_none() {
            self.focused_client = Some(client.window);
        }
        self.clients.insert(idx, client);
    }

    pub fn remove_client(&mut self, window: Window) -> Option<Client> {
        let idx = self.position(window)?;
        let client = self.clients.remove(idx);

        if self.focused_client == Some(window) {
            let next = match client.floating {
                Some(_) => idx.saturating_sub(1),
                None => idx,
            };

            self.focused_client = self
                .clients
                .get(next)
                .or_else(|| self.clients.last())
                .map(|c| c.window);
        }

        Some(client)
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub current_workspace: usize,
    pub last_workspace: usize,
}

impl WorkspaceManager {
    pub fn new(num_workspaces: usize) -> Self {
        Self {
            workspaces: (0..num_workspaces).map(Workspace::new).collect(),
            current_workspace: 0,
            last_workspace: 1,
        }
    }

    pub fn current(&self) -> &Workspace {
        &self.workspaces[self.current_workspace]
    }

    pub fn current_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.current_workspace]
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Workspace> {
        self.workspaces.get_mut(id)
    }

    pub fn contains(&self, id: usize) -> bool {
        id < self.workspaces.len()
    }
}

impl WindowManager {
    pub fn switch_to_workspace(&mut self, workspace_id: usize) -> Result<bool> {
        let workspaces = &self.monitors.current().workspaces;
        if !workspaces.contains(workspace_id) || workspaces.current_workspace == workspace_id {
            return Ok(false);
        }

        println!("Switching to workspace {}", workspace_id + 1);

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

    pub fn move_focused_to_workspace(&mut self, workspace_id: usize) -> Result<bool> {
        let workspaces = &self.monitors.current().workspaces;
        if !workspaces.contains(workspace_id) || workspaces.current_workspace == workspace_id {
            return Ok(false);
        }

        let Some(window) = self.focused_client() else {
            return Ok(false);
        };

        println!("Moving client {} to workspace {}", window, workspace_id + 1);

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
