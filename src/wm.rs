use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

use crate::alerts::Alert;
use crate::atoms::Atoms;
use crate::config::config::{BORDER_FOCUSED, BORDER_UNFOCUSED, BORDER_WIDTH};
use crate::keybindings::KeyBindingManager;
use crate::utils::run_autostart;
use crate::workspaces::WorkspaceManager;

pub struct WindowManager {
    pub conn: RustConnection,
    pub screen_num: usize,
    pub root: Window,
    pub border_width: u32,
    pub border_focused_color: u32,
    pub border_unfocused_color: u32,
    pub keybindings: KeyBindingManager,
    pub alerts: Vec<Alert>,
    pub workspaces: WorkspaceManager,
    pub atoms: Atoms,
}

impl WindowManager {
    pub fn new() -> Result<Self> {
        let (conn, screen_num) =
            RustConnection::connect(None).context("Failed to connect to X server")?;

        let setup = conn.setup();
        let screen = &setup.roots[screen_num];
        let root = screen.root;

        let change = ChangeWindowAttributesAux::default().event_mask(
            EventMask::SUBSTRUCTURE_REDIRECT
                | EventMask::SUBSTRUCTURE_NOTIFY
                | EventMask::BUTTON_PRESS
                | EventMask::ENTER_WINDOW
                | EventMask::STRUCTURE_NOTIFY
                | EventMask::PROPERTY_CHANGE,
        );

        conn.change_window_attributes(root, &change)?
            .check()
            .context("Another window manager is already running")?;

        conn.flush()?;

        let atoms = Atoms::new(&conn)?;

        Ok(Self {
            conn,
            screen_num,
            root,
            keybindings: KeyBindingManager::new(),
            alerts: Vec::new(),
            workspaces: WorkspaceManager::new(9),
            border_width: BORDER_WIDTH,
            border_focused_color: BORDER_FOCUSED,
            border_unfocused_color: BORDER_UNFOCUSED,
            atoms,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        run_autostart();

        loop {
            let event = self.conn.wait_for_event()?;

            match event {
                Event::KeyPress(e) => {
                    if let Err(err) = self.handle_key_press(&e) {
                        eprintln!("Error handling key press: {}", err);
                    }
                }
                Event::MapRequest(e) => {
                    if let Err(err) = self.manage_client(e) {
                        eprintln!("Error managing client: {}", err);
                    }
                }
                Event::UnmapNotify(e) => {
                    if let Err(err) = self.unmanage_client(e.window) {
                        eprintln!("Error unmanaging client: {}", err);
                    }
                }
                Event::DestroyNotify(e) => {
                    if let Err(err) = self.unmanage_client(e.window) {
                        eprintln!("Error on destroy notify: {}", err);
                    }
                }
                Event::EnterNotify(e) => {
                    if self.clients().contains_key(&e.event) {
                        if self.focused_client() != Some(e.event) {
                            self.set_focused_client(Some(e.event));

                            if let Err(err) =
                                self.conn
                                    .set_input_focus(InputFocus::PARENT, e.event, CURRENT_TIME)
                            {
                                eprintln!("Error setting focus: {}", err);
                            }

                            if let Err(err) = self.update_window_borders() {
                                eprintln!("Error updating borders: {}", err);
                            }

                            if let Err(err) = self.conn.flush() {
                                eprintln!("Error flushing: {}", err);
                            }
                        }
                    }
                }
                Event::Expose(e) => {
                    if let Some(alert) = self.alerts.iter().find(|a| a.window == e.window) {
                        if e.count == 0 {
                            if let Err(err) = self.redraw_alert(alert) {
                                eprintln!("Error redrawing alert: {}", err);
                            }
                        }
                    }
                }
                Event::ClientMessage(e) => {
                    if e.type_ == self.atoms.net_wm_state {
                        let data = e.data.as_data32();

                        let action = data[0];
                        let state1 = data[1];
                        let state2 = data[2];

                        // TODO: call a method to determine if the window is called as fullscreen or float

                        if let Err(err) =
                            self.handle_state_request(e.window, action, state1, state2)
                        {
                            eprintln!("Error handling state request: {}", err);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
