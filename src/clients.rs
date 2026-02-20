use anyhow::Result;
use x11rb::protocol::xproto::*;
use x11rb::CURRENT_TIME;
use x11rb::{connection::Connection, protocol::xproto::MapRequestEvent};

use crate::wm::WindowManager;

#[derive(Debug, Clone)]
pub struct ClientState {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub is_fullscreen: bool,
    pub saved_x: i16,
    pub saved_y: i16,
    pub saved_width: u16,
    pub saved_height: u16,
}

impl Default for ClientState {
    fn default() -> Self {
        ClientState {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            is_fullscreen: false,
            saved_x: 0,
            saved_y: 0,
            saved_width: 100,
            saved_height: 100,
        }
    }
}

impl ClientState {
    pub fn new(x: i16, y: i16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
            is_fullscreen: false,
            saved_x: x,
            saved_y: y,
            saved_width: width,
            saved_height: height,
        }
    }

    pub fn save_geometry(&mut self) {
        if !self.is_fullscreen {
            self.saved_x = self.x;
            self.saved_y = self.y;
            self.saved_width = self.width;
            self.saved_height = self.height;
        }
    }

    pub fn restore_geometry(&mut self) {
        self.x = self.saved_x;
        self.y = self.saved_y;
        self.width = self.saved_width;
        self.height = self.saved_height;
    }
}

impl WindowManager {
    pub fn clients(&self) -> &std::collections::HashMap<Window, ClientState> {
        &self.workspaces.current().clients
    }

    pub fn clients_mut(&mut self) -> &mut std::collections::HashMap<Window, ClientState> {
        &mut self.workspaces.current_mut().clients
    }

    pub fn focused_client(&self) -> Option<Window> {
        self.workspaces.current().focused_client
    }

    pub fn set_focused_client(&mut self, window: Option<Window>) {
        self.workspaces.current_mut().focused_client = window;
    }

    pub fn manage_client(&mut self, e: MapRequestEvent) -> Result<()> {
        let client = e.window;

        println!("Managing new client: {}", client);

        let initial_state = ClientState::default();

        self.workspaces
            .current_mut()
            .add_client(client, initial_state);

        self.conn.change_window_attributes(
            client,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::ENTER_WINDOW | EventMask::FOCUS_CHANGE)
                .border_pixel(self.border_unfocused_color),
        )?;

        let mut should_fullscreen = false;

        if let Ok(reply) = self
            .conn
            .get_property(
                false,
                client,
                self.atoms.net_wm_state,
                AtomEnum::ATOM,
                0,
                1024,
            )?
            .reply()
        {
            if let Some(atoms) = reply.value32() {
                for atom in atoms {
                    if atom == self.atoms.net_wm_state_fullscreen {
                        should_fullscreen = true;
                        break;
                    }
                }
            }
        }

        if should_fullscreen {
            self.conn.map_window(client)?;
            self.fullscreen_window(client)?;
            return Ok(());
        }

        self.conn.configure_window(
            client,
            &ConfigureWindowAux::new().border_width(self.border_width),
        )?;

        self.conn.map_window(client)?;

        self.set_focused_client(Some(client));
        self.conn
            .set_input_focus(InputFocus::PARENT, client, CURRENT_TIME)?;

        self.layout()?;

        self.update_window_borders()?;

        self.conn.flush()?;
        Ok(())
    }

    pub fn focus_next(&mut self) -> Result<()> {
        if self.clients().is_empty() {
            return Ok(());
        }

        let mut windows: Vec<Window> = self.clients().keys().copied().collect();
        windows.sort();

        if windows.len() == 1 {
            return Ok(());
        }

        let current_index = if let Some(current) = self.focused_client() {
            windows.iter().position(|&w| w == current).unwrap_or(0)
        } else {
            0
        };

        let next_index = (current_index + 1) % windows.len();
        let next_window = windows[next_index];

        self.set_focused_client(Some(next_window));

        self.conn
            .set_input_focus(InputFocus::PARENT, next_window, CURRENT_TIME)?;

        self.conn.configure_window(
            next_window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;

        self.update_window_borders()?;

        self.restack_alerts()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn focus_prev(&mut self) -> Result<()> {
        if self.clients().is_empty() {
            return Ok(());
        }

        let mut windows: Vec<Window> = self.clients().keys().copied().collect();
        windows.sort();

        if windows.len() == 1 {
            return Ok(());
        }

        let current_index = if let Some(current) = self.focused_client() {
            windows.iter().position(|&w| w == current).unwrap_or(0)
        } else {
            0
        };

        let prev_index = if current_index == 0 {
            windows.len() - 1
        } else {
            current_index - 1
        };

        let prev_window = windows[prev_index];

        self.set_focused_client(Some(prev_window));

        self.conn
            .set_input_focus(InputFocus::PARENT, prev_window, CURRENT_TIME)?;

        self.conn.configure_window(
            prev_window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;

        self.update_window_borders()?;

        self.restack_alerts()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn unmanage_client(&mut self, window: Window) -> Result<()> {
        println!("Unmanaging client: {}", window);

        self.workspaces.current_mut().remove_client(window);

        if self.focused_client() == Some(window) {
            if !self.clients().is_empty() {
                let next_window = *self.clients().keys().next().unwrap();
                self.set_focused_client(Some(next_window));

                self.conn
                    .set_input_focus(InputFocus::PARENT, next_window, CURRENT_TIME)?;
            } else {
                self.set_focused_client(None);
            }
        }

        self.layout()?;

        if !self.clients().is_empty() {
            self.update_window_borders()?;
        }

        self.restack_alerts()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn update_window_borders(&mut self) -> Result<()> {
        for &window in self.clients().keys() {
            let is_focused = self.focused_client() == Some(window);
            let color = if is_focused {
                self.border_focused_color
            } else {
                self.border_unfocused_color
            };

            self.conn.change_window_attributes(
                window,
                &ChangeWindowAttributesAux::new().border_pixel(color),
            )?;
        }

        self.restack_alerts()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn close_window(&mut self, window: Window) -> Result<()> {
        if self.window_supports_protocol(window, self.atoms.wm_delete_window)? {
            self.send_delete_window(window)?;
            println!("Sent WM_DELETE_WINDOW to window {}", window);
        } else {
            println!(
                "Window {} doesn't support WM_DELETE_WINDOW, forcing destroy",
                window
            );
            self.conn.destroy_window(window)?;
        }

        self.conn.flush()?;

        Ok(())
    }

    fn window_supports_protocol(&self, window: Window, protocol: Atom) -> Result<bool> {
        let protocols = match self
            .conn
            .get_property(
                false,
                window,
                self.atoms.wm_protocols,
                AtomEnum::ATOM,
                0,
                1024,
            )?
            .reply()
        {
            Ok(reply) => reply,
            Err(_) => return Ok(false),
        };

        if protocols.type_ != AtomEnum::ATOM.into() || protocols.format != 32 {
            return Ok(false);
        }

        let atoms: Vec<Atom> = protocols
            .value32()
            .ok_or_else(|| anyhow::anyhow!("Invalid property format"))?
            .collect();

        Ok(atoms.contains(&protocol))
    }

    fn send_delete_window(&self, window: Window) -> Result<()> {
        let event = ClientMessageEvent {
            response_type: CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window,
            type_: self.atoms.wm_protocols,
            data: ClientMessageData::from([
                self.atoms.wm_delete_window,
                x11rb::CURRENT_TIME,
                0,
                0,
                0,
            ]),
        };

        self.conn
            .send_event(false, window, EventMask::NO_EVENT, event)?;

        Ok(())
    }

    pub fn close_focused_window(&mut self) -> Result<()> {
        if let Some(window) = self.focused_client() {
            self.close_window(window)?;
        }
        Ok(())
    }

    pub fn toggle_fullscreen(&mut self, window: Window) -> Result<()> {
        if let Some(state) = self.clients_mut().get_mut(&window) {
            if state.is_fullscreen {
                self.unfullscreen_window(window)?;
            } else {
                self.fullscreen_window(window)?;
            }
        }

        Ok(())
    }

    pub fn fullscreen_window(&mut self, window: Window) -> Result<()> {
        println!("Setting window {} to fullscreen", window);

        let geometry = {
            let screen = self.conn.setup().roots.get(0).unwrap();
            (screen.width_in_pixels, screen.height_in_pixels)
        };

        let (width, height) = geometry;

        self.conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(0)
                .y(0)
                .width(width as u32)
                .height(height as u32)
                .border_width(0)
                .stack_mode(StackMode::ABOVE),
        )?;

        self.conn.change_property(
            PropMode::REPLACE,
            window,
            self.atoms.net_wm_state,
            AtomEnum::ATOM,
            8,
            1,
            &[self.atoms.net_wm_state_fullscreen as u8],
        )?;

        self.conn.flush()?;

        if let Some(state) = self.clients_mut().get_mut(&window) {
            state.save_geometry();
            state.is_fullscreen = true;

            state.x = 0;
            state.y = 0;
            state.width = width;
            state.height = height;
        }

        Ok(())
    }

    pub fn unfullscreen_window(&mut self, window: Window) -> Result<()> {
        println!("Removing fullscren from window {}", window);

        if let Some(state) = self.clients_mut().get_mut(&window) {
            state.is_fullscreen = false;

            let saved_x = state.saved_x;
            let saved_y = state.saved_y;
            let saved_width = state.saved_width;
            let saved_height = state.saved_height;

            state.restore_geometry();

            self.conn.configure_window(
                window,
                &ConfigureWindowAux::new()
                    .x(saved_x as i32)
                    .y(saved_y as i32)
                    .width(saved_width as u32)
                    .height(saved_height as u32)
                    .border_width(self.border_width),
            )?;

            self.conn.delete_property(window, self.atoms.net_wm_state)?;

            self.conn.flush()?;
            self.layout()?;
        }

        Ok(())
    }

    /// manage change state request
    /// action: 0 = remove, 1 = add, 2 = toggle
    pub fn handle_state_request(
        &mut self,
        window: Window,
        action: u32,
        state1: Atom,
        state2: Atom,
    ) -> Result<()> {
        if state1 == self.atoms.net_wm_state_fullscreen
            || state2 == self.atoms.net_wm_state_fullscreen
        {
            match action {
                0 => self.unfullscreen_window(window)?,
                1 => self.fullscreen_window(window)?,
                2 => self.toggle_fullscreen(window)?,
                _ => {}
            }
        }

        Ok(())
    }
}
