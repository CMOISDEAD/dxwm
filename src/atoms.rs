use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete_window: Atom,
    pub net_wm_state: Atom,
    pub net_wm_state_fullscreen: Atom,
    pub net_wm_name: Atom,
    pub net_wm_window_type: Atom,
    pub net_wm_window_type_dialog: Atom,
    pub net_wm_window_type_utility: Atom,
    pub net_wm_window_type_splash: Atom,
    pub net_wm_window_type_notification: Atom,
    pub utf8_string: Atom,
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
            net_wm_state: intern("_NET_WM_STATE")?,
            net_wm_state_fullscreen: intern("_NET_WM_STATE_FULLSCREEN")?,
            net_wm_name: intern("_NET_WM_NAME")?,
            net_wm_window_type: intern("_NET_WM_WINDOW_TYPE")?,
            net_wm_window_type_dialog: intern("_NET_WM_WINDOW_TYPE_DIALOG")?,
            net_wm_window_type_utility: intern("_NET_WM_WINDOW_TYPE_UTILITY")?,
            net_wm_window_type_splash: intern("_NET_WM_WINDOW_TYPE_SPLASH")?,
            net_wm_window_type_notification: intern("_NET_WM_WINDOW_TYPE_NOTIFICATION")?,
            utf8_string: intern("UTF8_STRING")?,
        })
    }
}
