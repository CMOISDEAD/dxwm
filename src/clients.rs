use anyhow::Result;
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use crate::config::DEFAULT_COLUMN_WIDTH;
use crate::wm::WindowManager;
use crate::workspaces::Workspace;

/// Frame geometry in root coordinates, border included
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

/// A managed top-level window, reparented into a frame that draws its border and
/// title bar. The geometry is the frame's last one set by the layout, border included
#[derive(Debug, Clone)]
pub struct Client {
    pub window: Window,
    pub frame: Window,
    pub title: String,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub fullscreen: bool,
    /// Floating clients keep this geometry instead of being tiled
    pub floating: Option<Rect>,
    /// Width while tiled, as a fraction of the monitor
    pub column_width: f32,
}

impl Client {
    fn new(window: Window, frame: Window, title: String, floating: Option<Rect>) -> Self {
        Self {
            window,
            frame,
            title,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            fullscreen: false,
            floating,
            column_width: DEFAULT_COLUMN_WIDTH,
        }
    }
}

impl WindowManager {
    /// Visible workspace of the current monitor
    pub fn workspace(&self) -> &Workspace {
        self.monitors.current().workspaces.current()
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        self.monitors.current_mut().workspaces.current_mut()
    }

    pub fn focused_client(&self) -> Option<Window> {
        self.workspace().focused_client
    }

    /// Find the monitor and workspace that own a client
    pub fn find_client(&self, window: Window) -> Option<(usize, u8)> {
        self.monitors
            .monitors
            .iter()
            .enumerate()
            .find_map(|(i, monitor)| {
                monitor
                    .workspaces
                    .workspaces
                    .iter()
                    .find(|ws| ws.get(window).is_some())
                    .map(|ws| (i, ws.id))
            })
    }

    /// Look up a client in any monitor/workspace
    pub fn client(&self, window: Window) -> Option<&Client> {
        self.monitors
            .monitors
            .iter()
            .flat_map(|m| m.workspaces.workspaces.iter())
            .find_map(|ws| ws.get(window))
    }

    pub fn client_by_frame(&self, frame: Window) -> Option<&Client> {
        self.monitors
            .monitors
            .iter()
            .flat_map(|m| m.workspaces.workspaces.iter())
            .flat_map(|ws| ws.clients.iter())
            .find(|c| c.frame == frame)
    }

    pub fn client_mut(&mut self, window: Window) -> Option<&mut Client> {
        self.monitors
            .monitors
            .iter_mut()
            .flat_map(|m| m.workspaces.workspaces.iter_mut())
            .find_map(|ws| ws.get_mut(window))
    }

    fn workspace_of_mut(&mut self, monitor_id: usize, workspace_id: u8) -> Option<&mut Workspace> {
        self.monitors
            .monitors
            .get_mut(monitor_id)?
            .workspaces
            .get_mut(workspace_id)
    }

    fn is_workspace_visible(&self, monitor_id: usize, workspace_id: u8) -> bool {
        self.monitors
            .get(monitor_id)
            .is_some_and(|m| m.workspaces.current_workspace == workspace_id)
    }

    /// Unmap the frame of a client. The client itself stays mapped inside, so this
    /// doesn't generate an UnmapNotify that could be mistaken for it withdrawing
    pub fn hide_client(&mut self, window: Window) -> Result<()> {
        if let Some(client) = self.client(window) {
            self.conn.unmap_window(client.frame)?;
        }
        Ok(())
    }

    pub fn show_client(&mut self, window: Window) -> Result<()> {
        if let Some(client) = self.client(window) {
            self.conn.map_window(client.frame)?;
        }
        Ok(())
    }

    /// Give X input focus to the focused client of the current monitor (or the root)
    pub fn focus_current(&mut self) -> Result<()> {
        match self.focused_client() {
            Some(window) => {
                self.conn
                    .set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)?;
                if let Some(client) = self.client(window) {
                    self.conn.configure_window(
                        client.frame,
                        &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                    )?;

                    if client.floating.is_none() {
                        self.raise_floating(self.monitors.current_monitor)?;
                    }
                }
            }
            None => {
                self.conn
                    .set_input_focus(InputFocus::POINTER_ROOT, self.root, CURRENT_TIME)?;
            }
        }

        self.ignore_pending_enters()?;
        self.update_client_borders()
    }

    /// Focus a client, switching to its monitor if needed
    pub fn set_focused_client(&mut self, window: Window) -> Result<()> {
        let Some((monitor_id, workspace_id)) = self.find_client(window) else {
            return Ok(());
        };

        if let Some(workspace) = self.workspace_of_mut(monitor_id, workspace_id) {
            workspace.focused_client = Some(window);
        }

        if monitor_id != self.monitors.current_monitor {
            self.focus_monitor(monitor_id, false)
        } else {
            self.focus_current()
        }
    }

    /// MapRequest: start managing a new window in the visible workspace of the current monitor
    pub fn manage_client(&mut self, e: MapRequestEvent) -> Result<()> {
        let window = e.window;

        // Already managed: it's inside its frame, which decides if it's visible
        if self.find_client(window).is_some() {
            self.conn.map_window(window)?;
            return Ok(());
        }

        println!("Managing new client: {}", window);

        self.conn.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;

        let title = self.read_title(window)?;
        let floating = self.initial_float_rect(window)?;
        let takes_focus = !self.is_notification(window)?;
        let frame = self.create_frame(window)?;
        self.conn.map_window(window)?;

        let workspace = self.workspace_mut();
        workspace.add_client(Client::new(window, frame, title, floating));
        if takes_focus {
            workspace.focused_client = Some(window);
        }

        if self.wants_fullscreen(window)? {
            self.set_fullscreen(window, true)?;
        } else {
            self.layout()?;
        }

        self.conn.map_window(frame)?;
        self.focus_current()
    }

    fn wants_fullscreen(&self, window: Window) -> Result<bool> {
        let reply = self
            .conn
            .get_property(
                false,
                window,
                self.atoms.net_wm_state,
                AtomEnum::ATOM,
                0,
                1024,
            )?
            .reply()?;

        Ok(reply
            .value32()
            .is_some_and(|mut atoms| atoms.any(|a| a == self.atoms.net_wm_state_fullscreen)))
    }

    /// UnmapNotify / DestroyNotify of a client: forget it and destroy its frame.
    /// `destroyed` is false for UnmapNotify events. Events about frames are ignored
    pub fn unmanage_client(&mut self, window: Window, destroyed: bool) -> Result<()> {
        let Some((monitor_id, workspace_id)) = self.find_client(window) else {
            return Ok(());
        };

        println!("Unmanaging client: {}", window);

        if let Some(client) = self
            .workspace_of_mut(monitor_id, workspace_id)
            .and_then(|ws| ws.remove_client(window))
        {
            self.destroy_frame(&client, destroyed)?;
        }

        if self.is_workspace_visible(monitor_id, workspace_id) {
            self.layout_monitor(monitor_id)?;
        }

        if monitor_id == self.monitors.current_monitor {
            self.focus_current()?;
        }

        Ok(())
    }

    pub fn focus_next(&mut self) -> Result<()> {
        self.focus_step(1)
    }

    pub fn focus_prev(&mut self) -> Result<()> {
        self.focus_step(-1)
    }

    /// Move the focus along the workspace, stopping at both ends
    fn focus_step(&mut self, step: isize) -> Result<()> {
        let workspace = self.workspace();
        let Some(current) = workspace.focused_client.and_then(|w| workspace.position(w)) else {
            return Ok(());
        };

        let target = current
            .saturating_add_signed(step)
            .min(workspace.clients.len() - 1);

        if target == current {
            return Ok(());
        }

        let window = workspace.clients[target].window;
        self.set_focused_client(window)?;
        self.layout()
    }

    pub fn swap_next(&mut self) -> Result<()> {
        self.swap_step(1)
    }

    pub fn swap_prev(&mut self) -> Result<()> {
        self.swap_step(-1)
    }

    /// Swap the focused client with a neighbour, stopping at both ends
    fn swap_step(&mut self, step: isize) -> Result<()> {
        let workspace = self.workspace_mut();

        let Some(current) = workspace.focused_client.and_then(|w| workspace.position(w)) else {
            return Ok(());
        };

        let target = current
            .saturating_add_signed(step)
            .min(workspace.clients.len() - 1);

        if target == current {
            return Ok(());
        }

        workspace.clients.swap(current, target);
        self.layout()
    }

    /// Only the focused client of the current monitor gets the focused decoration
    pub fn update_client_borders(&mut self) -> Result<()> {
        let focused = self.focused_client();

        for monitor in &self.monitors.monitors {
            for workspace in &monitor.workspaces.workspaces {
                for client in &workspace.clients {
                    self.draw_frame(client, Some(client.window) == focused)?;
                }
            }
        }

        self.restack_alerts()?;
        self.conn.flush()?;
        Ok(())
    }

    /// Ask the focused client to close with WM_DELETE_WINDOW, or kill it if it
    /// doesn't support the protocol
    pub fn close_focused_client(&mut self) -> Result<()> {
        let Some(window) = self.focused_client() else {
            return Ok(());
        };

        if self.supports_delete_window(window)? {
            println!("Sending WM_DELETE_WINDOW to client {}", window);

            let event = ClientMessageEvent::new(
                32,
                window,
                self.atoms.wm_protocols,
                [self.atoms.wm_delete_window, CURRENT_TIME, 0, 0, 0],
            );
            self.conn
                .send_event(false, window, EventMask::NO_EVENT, event)?;
        } else {
            println!(
                "Client {} doesn't support WM_DELETE_WINDOW, killing it",
                window
            );
            self.conn.kill_client(window)?;
        }

        self.conn.flush()?;
        Ok(())
    }

    fn supports_delete_window(&self, window: Window) -> Result<bool> {
        let reply = self
            .conn
            .get_property(
                false,
                window,
                self.atoms.wm_protocols,
                AtomEnum::ATOM,
                0,
                1024,
            )?
            .reply()?;

        Ok(reply
            .value32()
            .is_some_and(|mut atoms| atoms.any(|a| a == self.atoms.wm_delete_window)))
    }

    pub fn toggle_fullscreen(&mut self, window: Window) -> Result<()> {
        match self.client(window) {
            Some(client) => self.set_fullscreen(window, !client.fullscreen),
            None => Ok(()),
        }
    }

    /// Set or unset fullscreen, updating _NET_WM_STATE so the client knows
    pub fn set_fullscreen(&mut self, window: Window, fullscreen: bool) -> Result<()> {
        let Some((monitor_id, _)) = self.find_client(window) else {
            return Ok(());
        };

        if let Some(client) = self.client_mut(window) {
            client.fullscreen = fullscreen;
        }

        if fullscreen {
            println!("Setting client {} to fullscreen", window);
            self.conn.change_property32(
                PropMode::REPLACE,
                window,
                self.atoms.net_wm_state,
                AtomEnum::ATOM,
                &[self.atoms.net_wm_state_fullscreen],
            )?;
        } else {
            println!("Removing fullscreen from client {}", window);
            self.conn.delete_property(window, self.atoms.net_wm_state)?;
        }

        self.layout_monitor(monitor_id)
    }

    /// _NET_WM_STATE client message. Only fullscreen is supported
    pub fn handle_state_request(&mut self, e: ClientMessageEvent) -> Result<()> {
        let [action, first, second, ..] = e.data.as_data32();
        let fullscreen = self.atoms.net_wm_state_fullscreen;

        if first != fullscreen && second != fullscreen {
            return Ok(());
        }

        // _NET_WM_STATE_REMOVE = 0, _NET_WM_STATE_ADD = 1, _NET_WM_STATE_TOGGLE = 2
        match action {
            0 => self.set_fullscreen(e.window, false),
            1 => self.set_fullscreen(e.window, true),
            2 => self.toggle_fullscreen(e.window),
            _ => Ok(()),
        }
    }
}
