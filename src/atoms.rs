use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete_window: Atom,
    pub net_wm_state: Atom,
    pub net_wm_state_fullscreen: Atom,
    pub net_wm_name: Atom,
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
            utf8_string: intern("UTF8_STRING")?,
        })
    }
}
