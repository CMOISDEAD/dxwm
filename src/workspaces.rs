use anyhow::Result;
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::protocol::xproto::InputFocus;
use x11rb::protocol::xproto::Window;
use x11rb::CURRENT_TIME;

use crate::clients::ClientState;
use crate::layout::LayoutConfig;
use crate::wm::WindowManager;

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: u8,
    pub name: String,
    pub clients: HashMap<Window, ClientState>,
    pub focused_client: Option<Window>,
    pub layout_config: LayoutConfig,
}

impl Workspace {
    pub fn new(id: u8, name: String) -> Self {
        Self {
            id,
            name,
            clients: HashMap::new(),
            focused_client: None,
            layout_config: LayoutConfig::default(),
        }
    }

    /// Agrega un cliente al workspace
    pub fn add_client(&mut self, window: Window, state: ClientState) {
        self.clients.insert(window, state);
        if self.focused_client.is_none() {
            self.focused_client = Some(window);
        }
    }

    /// Remueve un cliente del workspace
    pub fn remove_client(&mut self, window: Window) -> Option<ClientState> {
        // Si era la focused, cambiar foco
        if self.focused_client == Some(window) {
            self.focused_client = self.clients.keys().find(|&&w| w != window).copied();
        }
        self.clients.remove(&window)
    }

    /// Verifica si el workspace está vacío
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    /// Obtiene el número de ventanas
    pub fn len(&self) -> usize {
        self.clients.len()
    }
}

pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub current_workspace: u8,
    pub last_workspace: u8,
}

impl WorkspaceManager {
    pub fn new(num_workspaces: u8) -> Self {
        let mut workspaces = Vec::new();

        for i in 1..=num_workspaces {
            workspaces.push(Workspace::new(i, format!("{}", i)));
        }

        Self {
            workspaces,
            current_workspace: 1,
            last_workspace: 1,
        }
    }

    /// Obtiene el workspace actual
    pub fn current(&self) -> &Workspace {
        &self.workspaces[(self.current_workspace - 1) as usize]
    }

    /// Obtiene el workspace actual (mutable)
    pub fn current_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[(self.current_workspace - 1) as usize]
    }

    /// Obtiene un workspace específico
    pub fn get(&self, id: u8) -> Option<&Workspace> {
        if id < 1 || id > self.workspaces.len() as u8 {
            return None;
        }
        Some(&self.workspaces[(id - 1) as usize])
    }

    /// Obtiene un workspace específico (mutable)
    pub fn get_mut(&mut self, id: u8) -> Option<&mut Workspace> {
        if id < 1 || id > self.workspaces.len() as u8 {
            return None;
        }
        Some(&mut self.workspaces[(id - 1) as usize])
    }

    /// Cambia al workspace especificado
    pub fn switch_to(&mut self, id: u8) -> bool {
        if id < 1 || id > self.workspaces.len() as u8 {
            return false;
        }
        if self.current_workspace != id {
            self.current_workspace = id;
            true
        } else {
            false
        }
    }

    pub fn move_window_to_workspace(&mut self, window: Window, target_workspace: u8) -> bool {
        if target_workspace < 1 || target_workspace > self.workspaces.len() as u8 {
            return false;
        }

        if let Some(state) = self.current_mut().remove_client(window) {
            if let Some(target) = self.get_mut(target_workspace) {
                target.add_client(window, state);
                return true;
            }
        }
        false
    }

    /// Obtiene el número total de workspaces
    pub fn count(&self) -> usize {
        self.workspaces.len()
    }
}

impl WindowManager {
    pub fn switch_to_workspace(&mut self, workspace_id: u8) -> Result<()> {
        if workspace_id < 1 || workspace_id > self.workspaces.count() as u8 {
            return Ok(());
        }

        if self.workspaces.current_workspace == workspace_id {
            return Ok(());
        }

        println!("Switching to workspace {}", workspace_id);
        self.workspaces.last_workspace = self.workspaces.current_workspace;

        for &window in self.clients().keys() {
            self.conn.unmap_window(window)?;
        }

        self.workspaces.switch_to(workspace_id);

        for &window in self.clients().keys() {
            self.conn.map_window(window)?;
        }

        if let Some(focused) = self.focused_client() {
            self.conn
                .set_input_focus(InputFocus::PARENT, focused, CURRENT_TIME)?;
        }

        self.layout()?;

        self.conn.flush()?;
        Ok(())
    }

    pub fn move_focused_to_workspace(&mut self, workspace_id: u8) -> Result<()> {
        if workspace_id < 1 || workspace_id > self.workspaces.count() as u8 {
            return Ok(());
        }

        if let Some(window) = self.focused_client() {
            println!("Moving window {} to workspace {}", window, workspace_id);

            self.conn.unmap_window(window)?;

            self.workspaces
                .move_window_to_workspace(window, workspace_id);

            self.layout()?;

            self.conn.flush()?;
        }

        Ok(())
    }

    pub fn cycle_last_workspace(&mut self) -> Result<()> {
        self.switch_to_workspace(self.workspaces.last_workspace);

        Ok(())
    }
}
