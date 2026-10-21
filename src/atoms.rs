use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use crate::wm::WindowManager;

pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete_window: Atom,
    pub wm_take_focus: Atom,
    pub wm_state: Atom,
    pub utf8_string: Atom,
    pub net_supported: Atom,
    pub net_supporting_wm_check: Atom,
    pub net_client_list: Atom,
    pub net_active_window: Atom,
    pub net_close_window: Atom,
    pub net_number_of_desktops: Atom,
    pub net_desktop_names: Atom,
    pub net_current_desktop: Atom,
    pub net_wm_desktop: Atom,
    pub net_wm_name: Atom,
    pub net_wm_state: Atom,
    pub net_wm_state_fullscreen: Atom,
    pub net_wm_state_demands_attention: Atom,
    pub net_wm_state_hidden: Atom,
    pub net_frame_extents: Atom,
    pub net_request_frame_extents: Atom,
    pub net_workarea: Atom,
    pub net_wm_strut: Atom,
    pub net_wm_strut_partial: Atom,
    pub net_wm_window_type: Atom,
    pub net_wm_window_type_dock: Atom,
    pub net_wm_window_type_dialog: Atom,
    pub net_wm_window_type_utility: Atom,
    pub net_wm_window_type_splash: Atom,
    pub net_wm_window_type_notification: Atom,
    pub dxwm_client: Atom,
    pub dxwm_monitors: Atom,
}

impl Atoms {
    pub fn new<C: Connection>(conn: &C) -> Result<Self> {
        let intern = |name: &str| -> Result<Atom> {
            Ok(conn
                .intern_atom(false, name.as_bytes())?
                .reply()
                .with_context(|| format!("Failed to intern {}", name))?
                .atom)
        };

        Ok(Self {
            wm_protocols: intern("WM_PROTOCOLS")?,
            wm_delete_window: intern("WM_DELETE_WINDOW")?,
            wm_take_focus: intern("WM_TAKE_FOCUS")?,
            wm_state: intern("WM_STATE")?,
            utf8_string: intern("UTF8_STRING")?,
            net_supported: intern("_NET_SUPPORTED")?,
            net_supporting_wm_check: intern("_NET_SUPPORTING_WM_CHECK")?,
            net_client_list: intern("_NET_CLIENT_LIST")?,
            net_active_window: intern("_NET_ACTIVE_WINDOW")?,
            net_close_window: intern("_NET_CLOSE_WINDOW")?,
            net_number_of_desktops: intern("_NET_NUMBER_OF_DESKTOPS")?,
            net_desktop_names: intern("_NET_DESKTOP_NAMES")?,
            net_current_desktop: intern("_NET_CURRENT_DESKTOP")?,
            net_wm_desktop: intern("_NET_WM_DESKTOP")?,
            net_wm_name: intern("_NET_WM_NAME")?,
            net_wm_state: intern("_NET_WM_STATE")?,
            net_wm_state_fullscreen: intern("_NET_WM_STATE_FULLSCREEN")?,
            net_wm_state_demands_attention: intern("_NET_WM_STATE_DEMANDS_ATTENTION")?,
            net_wm_state_hidden: intern("_NET_WM_STATE_HIDDEN")?,
            net_frame_extents: intern("_NET_FRAME_EXTENTS")?,
            net_request_frame_extents: intern("_NET_REQUEST_FRAME_EXTENTS")?,
            net_workarea: intern("_NET_WORKAREA")?,
            net_wm_strut: intern("_NET_WM_STRUT")?,
            net_wm_strut_partial: intern("_NET_WM_STRUT_PARTIAL")?,
            net_wm_window_type: intern("_NET_WM_WINDOW_TYPE")?,
            net_wm_window_type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            net_wm_window_type_dialog: intern("_NET_WM_WINDOW_TYPE_DIALOG")?,
            net_wm_window_type_utility: intern("_NET_WM_WINDOW_TYPE_UTILITY")?,
            net_wm_window_type_splash: intern("_NET_WM_WINDOW_TYPE_SPLASH")?,
            net_wm_window_type_notification: intern("_NET_WM_WINDOW_TYPE_NOTIFICATION")?,
            dxwm_client: intern("_DXWM_CLIENT")?,
            dxwm_monitors: intern("_DXWM_MONITORS")?,
        })
    }
}

impl WindowManager {
    /// Empty if the window doesn't have it or it isn't of type `kind`
    pub fn property32(
        &self,
        window: Window,
        property: impl Into<Atom>,
        kind: impl Into<Atom>,
    ) -> Result<Vec<u32>> {
        let reply = self
            .conn
            .get_property(false, window, property, kind, 0, 1024)?
            .reply()?;
        Ok(reply.value32().map(Iterator::collect).unwrap_or_default())
    }
}
