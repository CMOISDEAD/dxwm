use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::cursor::Handle as CursorHandle;
use x11rb::protocol::Event;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::alerts::Alerts;
use crate::atoms::Atoms;
use crate::bar::Bars;
use crate::config::NUM_WORKSPACES;
use crate::decorations::Decorations;
use crate::ewmh::Ewmh;
use crate::floating::Drag;
use crate::keybindings::KeyBindingManager;
use crate::keyboard::Keymap;
use crate::monitors::MonitorManager;
use crate::prompt::Prompt;
use crate::screenshot::Screenshot;

#[derive(Clone, Copy, PartialEq)]
pub enum Exit {
    Quit,
    Restart,
}

pub struct WindowManager {
    pub conn: RustConnection,
    pub root: Window,
    pub keybindings: KeyBindingManager,
    pub keymap: Keymap,
    pub alerts: Alerts,
    pub prompt: Option<Prompt>,
    pub monitors: MonitorManager,
    pub atoms: Atoms,
    pub decorations: Decorations,
    pub ewmh: Ewmh,
    pub bars: Bars,
    pub drag: Option<Drag>,
    pub screenshot: Option<Screenshot>,
    pub overlays: Vec<Window>,
    pub docks: Vec<Window>,
    pub enter_barrier: u64,
    pub exit: Option<Exit>,
}

impl WindowManager {
    pub fn new(restarted: bool) -> Result<Self> {
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

        // After a restart the server may not have dropped the old connection yet
        let mut attempts = if restarted { 40 } else { 0 };
        while let Err(err) = conn.change_window_attributes(root, &change)?.check() {
            if attempts == 0 {
                return Err(err).context("Another window manager is already running");
            }
            attempts -= 1;
            sleep(Duration::from_millis(50));
        }

        let resources = x11rb::resource_manager::new_from_default(&conn)?;
        let cursor = CursorHandle::new(&conn, screen_num, &resources)?
            .reply()?
            .load_cursor(&conn, "left_ptr")?;
        conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().cursor(cursor))?;

        let atoms = Atoms::new(&conn)?;
        let keymap = Keymap::new(&conn)?;
        let decorations = Decorations::new(&conn, root)?;
        let monitors = MonitorManager::detect(&conn, root, NUM_WORKSPACES)?;
        let ewmh = Ewmh::new(&conn, root, &atoms)?;

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
            keymap,
            alerts: Alerts::default(),
            prompt: None,
            monitors,
            atoms,
            decorations,
            ewmh,
            bars: Bars::default(),
            drag: None,
            screenshot: None,
            overlays: Vec::new(),
            docks: Vec::new(),
            enter_barrier: 0,
            exit: None,
        })
    }

    pub fn ignore_pending_enters(&mut self) -> Result<()> {
        self.enter_barrier = self.conn.no_operation()?.sequence_number();
        Ok(())
    }

    fn handle_enter_notify(&mut self, e: EnterNotifyEvent) -> Result<()> {
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

        let Some(window) = self.client_by_frame(e.event).map(|c| c.window) else {
            return Ok(());
        };

        if self.focused_client() != Some(window) {
            self.set_focused_client(window)?;
        }

        Ok(())
    }

    pub fn is_own_window(&self, window: Window) -> bool {
        self.alerts.window == Some(window)
            || self.prompt.as_ref().is_some_and(|p| p.window == window)
            || self.bars.contains(window)
    }

    pub fn quit(&mut self) -> Result<()> {
        self.close_prompt()?;
        self.exit = Some(Exit::Quit);
        Ok(())
    }

    pub fn run(&mut self) -> Result<Exit> {
        loop {
            if let Some(exit) = self.exit {
                return Ok(exit);
            }

            while let Some(event) = self.conn.poll_for_event()? {
                if let Err(err) = self.handle_event(event) {
                    eprintln!("Error handling event: {:#}", err);
                }
            }

            self.poll_alerts()?;
            self.poll_screenshot()?;
            self.poll_prompt()?;
            self.poll_bars()?;

            sleep(Duration::from_millis(32));
        }
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::KeyPress(e) if self.prompt.is_some() => self.handle_prompt_key(&e),
            Event::KeyPress(e) => self.handle_key_press(&e),
            Event::MapRequest(e) => self.manage_window(e.window),
            Event::MapNotify(e) => {
                if e.override_redirect
                    && !self.is_own_window(e.window)
                    && !self.overlays.contains(&e.window)
                {
                    self.overlays.push(e.window);
                }
                Ok(())
            }
            Event::UnmapNotify(e) => {
                self.overlays.retain(|&w| w != e.window);
                self.unmanage_dock(e.window)?;
                // Reparenting a mapped window into its frame unmaps it from the root
                if e.event == self.root {
                    return Ok(());
                }
                self.unmanage_client(e.window, false)
            }
            Event::DestroyNotify(e) => {
                self.overlays.retain(|&w| w != e.window);
                self.unmanage_dock(e.window)?;
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
                self.focus_monitor_at(e.root_x, e.root_y)
            }
            Event::Expose(e) if e.count == 0 => {
                if self.prompt.as_ref().is_some_and(|p| p.window == e.window) {
                    return self.draw_prompt();
                }
                if self.alerts.window == Some(e.window) {
                    return self.redraw_alerts();
                }
                if self.bars.contains(e.window) {
                    return self.redraw_bar(e.window);
                }
                self.redraw_frame(e.window)
            }
            Event::PropertyNotify(e) if self.docks.contains(&e.window) => {
                if e.atom == self.atoms.net_wm_strut || e.atom == self.atoms.net_wm_strut_partial {
                    self.update_struts()?;
                }
                Ok(())
            }
            Event::PropertyNotify(e)
                if e.atom == u32::from(AtomEnum::WM_HINTS) || e.atom == self.atoms.wm_protocols =>
            {
                self.update_focus_model(e.window)
            }
            Event::PropertyNotify(e) => self.handle_property_notify(e),
            Event::ClientMessage(e) => self.handle_client_message(e),
            Event::RandrScreenChangeNotify(_) | Event::RandrNotify(_) => self.refresh_monitors(),
            Event::Error(e) => {
                eprintln!("X11 error: {:?}", e);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
