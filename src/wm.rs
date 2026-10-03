use std::time::Duration;

use anyhow::{Context, Result};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::Event;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::alerts::Alert;
use crate::atoms::Atoms;
use crate::config::NUM_WORKSPACES;
use crate::decorations::Decorations;
use crate::floating::Drag;
use crate::keybindings::KeyBindingManager;
use crate::monitors::MonitorManager;
use crate::screenshot::Screenshot;
use crate::utils::run_autostart;

pub struct WindowManager {
    pub conn: RustConnection,
    pub root: Window,
    pub keybindings: KeyBindingManager,
    pub alerts: Vec<Alert>,
    pub monitors: MonitorManager,
    pub atoms: Atoms,
    pub decorations: Decorations,
    pub drag: Option<Drag>,
    pub screenshot: Option<Screenshot>,
    /// Mapped override-redirect windows of other programs (notifications, menus,
    /// tooltips). We don't manage them, only keep them above the clients
    pub overlays: Vec<Window>,
    /// EnterNotify events older than this request were caused by our own
    /// configure/map/warp requests, not by the user moving the mouse
    pub enter_barrier: u64,
}

impl WindowManager {
    /// Connect to the X server and become its window manager
    pub fn new() -> Result<Self> {
        let (conn, screen_num) =
            RustConnection::connect(None).context("Failed to connect to X server")?;

        let root = conn.setup().roots[screen_num].root;

        let change = ChangeWindowAttributesAux::default().event_mask(
            EventMask::SUBSTRUCTURE_REDIRECT
                | EventMask::SUBSTRUCTURE_NOTIFY
                | EventMask::ENTER_WINDOW
                | EventMask::POINTER_MOTION
                | EventMask::STRUCTURE_NOTIFY,
        );

        // Only one client can select SubstructureRedirect on the root
        conn.change_window_attributes(root, &change)?
            .check()
            .context("Another window manager is already running")?;

        let atoms = Atoms::new(&conn)?;
        let decorations = Decorations::new(&conn, root)?;
        // Also negotiates the RandR version, must happen before selecting its events
        let monitors = MonitorManager::detect(&conn, root, NUM_WORKSPACES)?;

        if conn
            .extension_information(randr::X11_EXTENSION_NAME)?
            .is_some()
        {
            conn.randr_select_input(
                root,
                randr::NotifyMask::SCREEN_CHANGE | randr::NotifyMask::CRTC_CHANGE,
            )?;
        }

        conn.flush()?;

        Ok(Self {
            conn,
            root,
            keybindings: KeyBindingManager::default(),
            alerts: Vec::new(),
            monitors,
            atoms,
            decorations,
            drag: None,
            screenshot: None,
            overlays: Vec::new(),
            enter_barrier: 0,
        })
    }

    /// Mark every EnterNotify generated up to now as stale, so windows moving under
    /// a still pointer don't steal the focus
    pub fn ignore_pending_enters(&mut self) -> Result<()> {
        self.enter_barrier = self.conn.no_operation()?.sequence_number();
        Ok(())
    }

    /// Focus follows the mouse, across clients and empty monitors
    fn handle_enter_notify(&mut self, e: EnterNotifyEvent) -> Result<()> {
        // Events carry the 16 lower bits of the last request processed by the server
        let stale = (e.sequence.wrapping_sub(self.enter_barrier as u16) as i16) < 0;
        if stale {
            return Ok(());
        }

        if e.event == self.root {
            return self.focus_monitor_at(e.root_x, e.root_y);
        }

        if e.mode != NotifyMode::NORMAL || e.detail == NotifyDetail::INFERIOR {
            return Ok(());
        }

        // Clients are inside frames, those are the ones that get the event
        let Some(window) = self.client_by_frame(e.event).map(|c| c.window) else {
            return Ok(());
        };

        if self.focused_client() != Some(window) {
            self.set_focused_client(window)?;
        }

        Ok(())
    }

    /// Main loop. Events are polled instead of waited for so alerts can expire
    /// and background screenshots can be followed
    pub fn run(&mut self) -> Result<()> {
        run_autostart();

        loop {
            while let Some(event) = self.conn.poll_for_event()? {
                if let Err(err) = self.handle_event(event) {
                    eprintln!("Error handling event: {:#}", err);
                }
            }

            self.clear_old_alerts()?;
            self.poll_screenshot()?;

            std::thread::sleep(Duration::from_millis(32));
        }
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::KeyPress(e) => self.handle_key_press(&e),
            Event::MapRequest(e) => self.manage_client(e),
            Event::MapNotify(e) => {
                let ours = self.alerts.iter().any(|a| a.window == e.window);
                if e.override_redirect && !ours && !self.overlays.contains(&e.window) {
                    self.overlays.push(e.window);
                }
                Ok(())
            }
            Event::UnmapNotify(e) => {
                self.overlays.retain(|&w| w != e.window);
                self.unmanage_client(e.window, false)
            }
            Event::DestroyNotify(e) => {
                self.overlays.retain(|&w| w != e.window);
                self.unmanage_client(e.window, true)
            }
            Event::EnterNotify(e) => self.handle_enter_notify(e),
            Event::ConfigureRequest(e) => self.handle_configure_request(e),
            Event::ButtonPress(e) => self.handle_button_press(e),
            Event::ButtonRelease(_) => self.handle_button_release(),
            Event::MotionNotify(e) => {
                if self.handle_drag_motion(&e)? {
                    return Ok(());
                }
                // Only reaches us while the pointer is over the root (empty monitor area)
                self.focus_monitor_at(e.root_x, e.root_y)
            }
            Event::Expose(e) if e.count == 0 => {
                match self.alerts.iter().find(|a| a.window == e.window) {
                    Some(alert) => self.redraw_alert(alert),
                    None => self.redraw_frame(e.window),
                }
            }
            Event::PropertyNotify(e) => self.handle_property_notify(e),
            Event::ClientMessage(e) if e.type_ == self.atoms.net_wm_state => {
                self.handle_state_request(e)
            }
            // Outputs changed (e.g. the user ran xrandr)
            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => self.refresh_monitors(),
            Event::Error(e) => {
                eprintln!("X11 error: {:?}", e);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
