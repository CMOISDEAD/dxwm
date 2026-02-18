use anyhow::Result;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageData, ClientMessageEvent,
    ConfigureWindowAux, ConnectionExt, EventMask, InputFocus, StackMode, Window,
    CLIENT_MESSAGE_EVENT,
};
use x11rb::CURRENT_TIME;
use x11rb::{connection::Connection, protocol::xproto::MapRequestEvent};

use crate::wm::WindowManager;

#[derive(Debug, Clone)]
pub struct ClientState {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub workspace: u16,
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

        let initial_state = ClientState {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
            workspace: self.current_workspace,
        };

        self.workspaces
            .current_mut()
            .add_client(client, initial_state);

        self.conn.change_window_attributes(
            client,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::ENTER_WINDOW | EventMask::FOCUS_CHANGE)
                .border_pixel(self.border_unfocused_color),
        )?;

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

    // focus_next corregido con ciclo y bordes
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

        // Ciclar: si estamos en la última, volver a la primera
        let next_index = (current_index + 1) % windows.len();
        let next_window = windows[next_index];

        self.set_focused_client(Some(next_window));

        self.conn
            .set_input_focus(InputFocus::PARENT, next_window, CURRENT_TIME)?;

        // Traer ventana al frente
        self.conn.configure_window(
            next_window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;

        // Actualizar bordes
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
}
