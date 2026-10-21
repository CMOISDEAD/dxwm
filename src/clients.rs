use anyhow::Result;
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::properties::WmHints;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use crate::config::{DEFAULT_COLUMN_WIDTH, MIN_COLUMN_WIDTH, Rule, SCRATCHPAD_CLASS};
use crate::wm::WindowManager;
use crate::workspaces::Workspace;

/// Frame geometry in root coordinates, border included
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn contains(&self, x: i16, y: i16) -> bool {
        let (x, y) = (x as i32, y as i32);
        x >= self.x as i32
            && x < self.x as i32 + self.width as i32
            && y >= self.y as i32
            && y < self.y as i32 + self.height as i32
    }

    pub fn center(&self) -> (i16, i16) {
        (
            self.x.saturating_add((self.width / 2) as i16),
            self.y.saturating_add((self.height / 2) as i16),
        )
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub window: Window,
    pub frame: Window,
    pub title: String,
    pub geometry: Rect,
    pub fullscreen: bool,
    pub floating: Option<Rect>,
    /// Fraction of the monitor width while tiled
    pub column_width: f32,
    /// ICCCM focus model: `WM_HINTS` input and `WM_TAKE_FOCUS` in `WM_PROTOCOLS`
    pub input: bool,
    pub take_focus: bool,
    pub urgent: bool,
    pub scratchpad: bool,
    /// Shares the column of the previous tiled client
    pub stacked: bool,
}

impl Client {
    pub fn new(window: Window, frame: Window, title: String, floating: Option<Rect>) -> Self {
        Self {
            window,
            frame,
            title,
            geometry: Rect::default(),
            fullscreen: false,
            floating,
            column_width: DEFAULT_COLUMN_WIDTH,
            input: true,
            take_focus: false,
            urgent: false,
            scratchpad: false,
            stacked: false,
        }
    }

    pub fn is_tiled(&self) -> bool {
        !self.fullscreen && self.floating.is_none()
    }
}

impl WindowManager {
    pub fn workspace(&self) -> &Workspace {
        self.monitors.current().workspaces.current()
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        self.monitors.current_mut().workspaces.current_mut()
    }

    pub fn focused_client(&self) -> Option<Window> {
        self.workspace().focused_client
    }

    pub fn find_client(&self, window: Window) -> Option<(usize, usize)> {
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

    pub fn workspace_of_mut(
        &mut self,
        monitor_id: usize,
        workspace_id: usize,
    ) -> Option<&mut Workspace> {
        self.monitors
            .monitors
            .get_mut(monitor_id)?
            .workspaces
            .get_mut(workspace_id)
    }

    pub fn is_workspace_visible(&self, monitor_id: usize, workspace_id: usize) -> bool {
        self.monitors
            .get(monitor_id)
            .is_some_and(|m| m.workspaces.current_workspace == workspace_id)
    }

    /// Only the frame is unmapped: an UnmapNotify of the client would look like
    /// it withdrawing
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

    pub fn focus_current(&mut self) -> Result<()> {
        match self.focused_client() {
            Some(window) => {
                if self.client(window).is_some_and(|c| c.urgent) {
                    self.set_urgent(window, false)?;
                }
                if let Some(client) = self.client(window) {
                    if client.input {
                        self.conn
                            .set_input_focus(InputFocus::PARENT, window, CURRENT_TIME)?;
                    }
                    if client.take_focus {
                        self.send_protocol(window, self.atoms.wm_take_focus)?;
                    }
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
        self.sync_ewmh()?;
        self.update_bars()?;
        self.update_client_borders()
    }

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

    pub fn jump_to_client(&mut self, window: Window) -> Result<()> {
        let Some((monitor_id, workspace_id)) = self.find_client(window) else {
            return Ok(());
        };
        let other_monitor = monitor_id != self.monitors.current_monitor;

        self.focus_monitor(monitor_id, false)?;
        self.switch_to_workspace(workspace_id)?;
        self.set_focused_client(window)?;
        self.layout()?;

        if other_monitor {
            self.warp_pointer_to_focus()?;
        }
        Ok(())
    }

    pub fn manage_window(&mut self, window: Window) -> Result<()> {
        // Its frame decides if it's visible
        let hidden_scratchpad = self.scratchpad.as_ref().is_some_and(|c| c.window == window);
        if hidden_scratchpad || self.find_client(window).is_some() {
            self.conn.map_window(window)?;
            return Ok(());
        }

        if self.is_dock(window)? {
            return self.manage_dock(window);
        }

        println!("Managing new client: {}", window);

        let takes_focus = !self.is_notification(window)?;
        let fullscreen = self.wants_fullscreen(window)?;
        let rule = self.rule_for(window)?;
        let client = self.new_client(window, rule)?;
        let frame = client.frame;

        let workspaces = &mut self.monitors.current_mut().workspaces;
        let target = rule
            .and_then(|r| r.workspace)
            .and_then(|number| number.checked_sub(1))
            .filter(|&id| workspaces.contains(id))
            .unwrap_or(workspaces.current_workspace);
        let visible = target == workspaces.current_workspace;

        let Some(workspace) = workspaces.get_mut(target) else {
            return Ok(());
        };
        workspace.add_client(client);
        if takes_focus {
            workspace.focused_client = Some(window);
        }

        if fullscreen {
            self.set_fullscreen(window, true)?;
        } else {
            self.layout()?;
        }

        if visible {
            self.conn.map_window(frame)?;
        }
        self.focus_current()
    }

    pub fn new_client(&mut self, window: Window, rule: Option<&Rule>) -> Result<Client> {
        self.conn.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;

        let title = self.read_title(window)?;
        let floating = self.initial_float_rect(window, rule.and_then(|r| r.floating))?;
        let frame = self.create_frame(window)?;
        self.conn.map_window(window)?;

        let mut client = Client::new(window, frame, title, floating);
        (client.input, client.take_focus) = self.read_focus_model(window)?;
        if let Some(width) = rule.and_then(|r| r.column_width) {
            client.column_width = width.clamp(MIN_COLUMN_WIDTH, 1.0);
        }
        if self.has_class(window, SCRATCHPAD_CLASS)? {
            client.scratchpad = true;
            client.floating = Some(self.scratchpad_rect());
        }
        Ok(client)
    }

    fn read_wm_hints(&self, window: Window) -> Result<Option<WmHints>> {
        Ok(WmHints::get(&self.conn, window)?.reply().ok().flatten())
    }

    fn read_focus_model(&self, window: Window) -> Result<(bool, bool)> {
        let input = self
            .read_wm_hints(window)?
            .and_then(|hints| hints.input)
            .unwrap_or(true);
        let take_focus = self.protocols(window)?.contains(&self.atoms.wm_take_focus);
        Ok((input, take_focus))
    }

    pub fn update_hints(&mut self, window: Window) -> Result<()> {
        if self.client(window).is_none() {
            return Ok(());
        }

        let (input, take_focus) = self.read_focus_model(window)?;
        let urgent = self
            .read_wm_hints(window)?
            .is_some_and(|hints| hints.urgent);
        if let Some(client) = self.client_mut(window) {
            (client.input, client.take_focus) = (input, take_focus);
        }
        self.set_urgent(window, urgent)
    }

    pub fn set_urgent(&mut self, window: Window, urgent: bool) -> Result<()> {
        let urgent = urgent && self.focused_client() != Some(window);
        let Some((monitor_id, workspace_id)) = self.find_client(window) else {
            return Ok(());
        };
        let Some(client) = self.client_mut(window) else {
            return Ok(());
        };
        if client.urgent == urgent {
            return Ok(());
        }

        client.urgent = urgent;
        let title = client.title.clone();
        self.set_state(window, self.atoms.net_wm_state_demands_attention, urgent)?;

        if urgent {
            let number = workspace_id + 1;
            let alert = if self.monitors.count() > 1 {
                format!("[URGENT] {}:{} {}", monitor_id, number, title)
            } else {
                format!("[URGENT] {} {}", number, title)
            };
            self.draw_alert(alert)?;
        }
        self.update_bars()?;
        self.update_client_borders()
    }

    pub fn set_state(&self, window: Window, atom: Atom, enabled: bool) -> Result<()> {
        let mut state = self.property32(window, self.atoms.net_wm_state, AtomEnum::ATOM)?;
        state.retain(|&a| a != atom);
        if enabled {
            state.push(atom);
        }

        self.conn.change_property32(
            PropMode::REPLACE,
            window,
            self.atoms.net_wm_state,
            AtomEnum::ATOM,
            &state,
        )?;
        Ok(())
    }

    pub fn wants_fullscreen(&self, window: Window) -> Result<bool> {
        Ok(self
            .property32(window, self.atoms.net_wm_state, AtomEnum::ATOM)?
            .contains(&self.atoms.net_wm_state_fullscreen))
    }

    pub fn unmanage_client(&mut self, window: Window, destroyed: bool) -> Result<()> {
        if self.scratchpad.as_ref().is_some_and(|c| c.window == window)
            && let Some(client) = self.scratchpad.take()
        {
            return self.destroy_frame(&client, destroyed);
        }

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
            self.focus_current()
        } else {
            self.sync_ewmh()?;
            self.update_bars()
        }
    }

    pub fn focus_next(&mut self) -> Result<()> {
        self.focus_step(1)
    }

    pub fn focus_prev(&mut self) -> Result<()> {
        self.focus_step(-1)
    }

    fn focus_step(&mut self, step: isize) -> Result<()> {
        let workspace = self.workspace();
        let Some((_, target)) = workspace.step_from_focused(step) else {
            return Ok(());
        };

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

    fn swap_step(&mut self, step: isize) -> Result<()> {
        let Some(window) = self.focused_client() else {
            return Ok(());
        };
        if self.workspace_mut().move_window(window, step > 0) {
            self.layout()?;
        }
        Ok(())
    }

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

    pub fn close_focused_client(&mut self) -> Result<()> {
        match self.focused_client() {
            Some(window) => self.close_client(window),
            None => Ok(()),
        }
    }

    pub fn close_client(&mut self, window: Window) -> Result<()> {
        if self.client(window).is_none() {
            return Ok(());
        }

        if self
            .protocols(window)?
            .contains(&self.atoms.wm_delete_window)
        {
            println!("Sending WM_DELETE_WINDOW to client {}", window);
            self.send_protocol(window, self.atoms.wm_delete_window)?;
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

    fn protocols(&self, window: Window) -> Result<Vec<Atom>> {
        self.property32(window, self.atoms.wm_protocols, AtomEnum::ATOM)
    }

    fn send_protocol(&self, window: Window, protocol: Atom) -> Result<()> {
        let event = ClientMessageEvent::new(
            32,
            window,
            self.atoms.wm_protocols,
            [protocol, CURRENT_TIME, 0, 0, 0],
        );
        self.conn
            .send_event(false, window, EventMask::NO_EVENT, event)?;
        Ok(())
    }

    pub fn toggle_fullscreen(&mut self, window: Window) -> Result<()> {
        match self.client(window) {
            Some(client) => self.set_fullscreen(window, !client.fullscreen),
            None => Ok(()),
        }
    }

    pub fn set_fullscreen(&mut self, window: Window, fullscreen: bool) -> Result<()> {
        let Some((monitor_id, _)) = self.find_client(window) else {
            return Ok(());
        };

        if let Some(client) = self.client_mut(window) {
            client.fullscreen = fullscreen;
        }

        if fullscreen {
            println!("Setting client {} to fullscreen", window);
        } else {
            println!("Removing fullscreen from client {}", window);
        }

        self.set_state(window, self.atoms.net_wm_state_fullscreen, fullscreen)?;
        self.layout_monitor(monitor_id)?;
        self.sync_ewmh()
    }

    pub fn handle_state_request(&mut self, e: ClientMessageEvent) -> Result<()> {
        let [action, first, second, ..] = e.data.as_data32();
        let Some(client) = self.client(e.window) else {
            return Ok(());
        };
        let (fullscreen, urgent) = (client.fullscreen, client.urgent);

        // _NET_WM_STATE_REMOVE, _NET_WM_STATE_ADD, _NET_WM_STATE_TOGGLE
        let apply = |current: bool| match action {
            0 => Some(false),
            1 => Some(true),
            2 => Some(!current),
            _ => None,
        };
        let requested = |atom: Atom| first == atom || second == atom;

        if requested(self.atoms.net_wm_state_fullscreen)
            && let Some(enabled) = apply(fullscreen)
        {
            self.set_fullscreen(e.window, enabled)?;
        }
        if requested(self.atoms.net_wm_state_demands_attention)
            && let Some(enabled) = apply(urgent)
        {
            self.set_urgent(e.window, enabled)?;
        }
        Ok(())
    }
}
