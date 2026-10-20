use anyhow::Result;
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use crate::atoms::Atoms;
use crate::config::NUM_WORKSPACES;
use crate::monitors::Reserved;
use crate::wm::WindowManager;

#[derive(Default)]
pub struct Ewmh {
    clients: Vec<(Window, u32)>,
    active: Option<Window>,
    desktop: Option<u32>,
}

impl Ewmh {
    pub fn new<C: Connection>(conn: &C, root: Window, atoms: &Atoms) -> Result<Self> {
        let check = conn.generate_id()?;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            check,
            root,
            -1,
            -1,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new(),
        )?;

        for window in [root, check] {
            conn.change_property32(
                PropMode::REPLACE,
                window,
                atoms.net_supporting_wm_check,
                AtomEnum::WINDOW,
                &[check],
            )?;
        }
        conn.change_property8(
            PropMode::REPLACE,
            check,
            atoms.net_wm_name,
            atoms.utf8_string,
            b"dxwm",
        )?;

        conn.change_property32(
            PropMode::REPLACE,
            root,
            atoms.net_supported,
            AtomEnum::ATOM,
            &[
                atoms.net_supporting_wm_check,
                atoms.net_client_list,
                atoms.net_active_window,
                atoms.net_close_window,
                atoms.net_number_of_desktops,
                atoms.net_desktop_names,
                atoms.net_current_desktop,
                atoms.net_wm_desktop,
                atoms.net_wm_name,
                atoms.net_wm_state,
                atoms.net_wm_state_fullscreen,
                atoms.net_wm_strut,
                atoms.net_wm_strut_partial,
                atoms.net_wm_window_type,
                atoms.net_wm_window_type_dock,
                atoms.net_wm_window_type_dialog,
                atoms.net_wm_window_type_utility,
                atoms.net_wm_window_type_splash,
                atoms.net_wm_window_type_notification,
            ],
        )?;

        conn.change_property32(
            PropMode::REPLACE,
            root,
            atoms.net_number_of_desktops,
            AtomEnum::CARDINAL,
            &[NUM_WORKSPACES as u32],
        )?;
        let names: Vec<u8> = (1..=NUM_WORKSPACES)
            .flat_map(|id| format!("{}\0", id).into_bytes())
            .collect();
        conn.change_property8(
            PropMode::REPLACE,
            root,
            atoms.net_desktop_names,
            atoms.utf8_string,
            &names,
        )?;

        Ok(Self::default())
    }
}

impl WindowManager {
    /// Desktops are the workspaces of the current monitor, each monitor has its own
    pub fn sync_ewmh(&mut self) -> Result<()> {
        let clients: Vec<(Window, u32)> = self
            .monitors
            .monitors
            .iter()
            .flat_map(|m| m.workspaces.workspaces.iter())
            .flat_map(|ws| ws.clients.iter().map(|c| (c.window, ws.id as u32 - 1)))
            .collect();

        if clients != self.ewmh.clients {
            let windows: Vec<Window> = clients.iter().map(|&(window, _)| window).collect();
            self.conn.change_property32(
                PropMode::REPLACE,
                self.root,
                self.atoms.net_client_list,
                AtomEnum::WINDOW,
                &windows,
            )?;

            for &(window, desktop) in &clients {
                if !self.ewmh.clients.contains(&(window, desktop)) {
                    self.conn.change_property32(
                        PropMode::REPLACE,
                        window,
                        self.atoms.net_wm_desktop,
                        AtomEnum::CARDINAL,
                        &[desktop],
                    )?;
                }
            }
            self.ewmh.clients = clients;
        }

        let active = self.focused_client();
        if active != self.ewmh.active {
            self.conn.change_property32(
                PropMode::REPLACE,
                self.root,
                self.atoms.net_active_window,
                AtomEnum::WINDOW,
                &[active.unwrap_or(x11rb::NONE)],
            )?;
            self.ewmh.active = active;
        }

        let desktop = self.workspace().id as u32 - 1;
        if Some(desktop) != self.ewmh.desktop {
            self.conn.change_property32(
                PropMode::REPLACE,
                self.root,
                self.atoms.net_current_desktop,
                AtomEnum::CARDINAL,
                &[desktop],
            )?;
            self.ewmh.desktop = Some(desktop);
        }

        Ok(())
    }

    pub fn handle_client_message(&mut self, e: ClientMessageEvent) -> Result<()> {
        let atoms = &self.atoms;

        if e.type_ == atoms.net_wm_state {
            self.handle_state_request(e)
        } else if e.type_ == atoms.net_active_window {
            self.jump_to_client(e.window)
        } else if e.type_ == atoms.net_close_window {
            self.close_client(e.window)
        } else if e.type_ == atoms.net_current_desktop {
            let desktop = e.data.as_data32()[0];
            if let Some(id) = desktop.checked_add(1).and_then(|id| u8::try_from(id).ok()) {
                self.switch_to_workspace(id)?;
            }
            Ok(())
        } else {
            Ok(())
        }
    }

    pub fn is_dock(&self, window: Window) -> Result<bool> {
        self.has_window_type(window, &[self.atoms.net_wm_window_type_dock])
    }

    pub fn manage_dock(&mut self, window: Window) -> Result<()> {
        if !self.docks.contains(&window) {
            self.conn.change_window_attributes(
                window,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )?;
            self.docks.push(window);
        }

        self.conn.map_window(window)?;
        self.update_struts()
    }

    pub fn unmanage_dock(&mut self, window: Window) -> Result<()> {
        if !self.docks.contains(&window) {
            return Ok(());
        }

        self.docks.retain(|&w| w != window);
        self.update_struts()
    }

    pub fn update_struts(&mut self) -> Result<()> {
        let screen = self.conn.get_geometry(self.root)?.reply()?;
        let (screen_width, screen_height) = (screen.width as i64, screen.height as i64);

        let mut reserved = vec![Reserved::default(); self.monitors.count()];

        for &dock in &self.docks {
            let Some(strut) = self.read_strut(dock)? else {
                continue;
            };
            let [left, right, top, bottom] =
                [strut[0], strut[1], strut[2], strut[3]].map(i64::from);
            let span = |i: usize| (strut[i] as i64, strut[i + 1] as i64);

            for (monitor, reserved) in self.monitors.monitors.iter().zip(&mut reserved) {
                let g = monitor.geometry;
                let (x, y) = (g.x as i64, g.y as i64);
                let (width, height) = (g.width as i64, g.height as i64);
                let overlaps = |(start, end): (i64, i64), from: i64, size: i64| {
                    start < from + size && end >= from
                };
                let fit = |amount: i64, size: i64| amount.clamp(0, size) as u16;

                if left > 0 && overlaps(span(4), y, height) {
                    reserved.left = reserved.left.max(fit(left - x, width));
                }
                if right > 0 && overlaps(span(6), y, height) {
                    reserved.right = reserved
                        .right
                        .max(fit(x + width - (screen_width - right), width));
                }
                if top > 0 && overlaps(span(8), x, width) {
                    reserved.top = reserved.top.max(fit(top - y, height));
                }
                if bottom > 0 && overlaps(span(10), x, width) {
                    reserved.bottom = reserved
                        .bottom
                        .max(fit(y + height - (screen_height - bottom), height));
                }
            }
        }

        for (monitor, reserved) in self.monitors.monitors.iter_mut().zip(reserved) {
            monitor.reserved = reserved;
        }

        self.layout_all_monitors()?;
        self.place_bars()
    }

    fn read_strut(&self, window: Window) -> Result<Option<[u32; 12]>> {
        for (property, length) in [
            (self.atoms.net_wm_strut_partial, 12),
            (self.atoms.net_wm_strut, 4),
        ] {
            let values = self.property32(window, property, AtomEnum::CARDINAL)?;
            if values.len() < length {
                continue;
            }

            let mut strut = [
                0,
                0,
                0,
                0,
                0,
                u32::MAX,
                0,
                u32::MAX,
                0,
                u32::MAX,
                0,
                u32::MAX,
            ];
            strut[..length].copy_from_slice(&values[..length]);
            return Ok(Some(strut));
        }

        Ok(None)
    }
}
