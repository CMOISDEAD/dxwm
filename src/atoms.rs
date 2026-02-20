use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

#[allow(dead_code)]
pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete_window: Atom,
    pub wm_state: Atom,
    pub wm_take_focus: Atom,
    pub net_wm_state: Atom,
    pub net_wm_state_fullscreen: Atom,
    pub net_active_window: Atom,
}

impl Atoms {
    pub fn new<C: Connection>(conn: &C) -> Result<Self> {
        let wm_protocols = conn
            .intern_atom(false, b"WM_PROTOCOLS")?
            .reply()
            .context("Failed to intern WM_PROTOCOLS")?
            .atom;

        let wm_delete_window = conn
            .intern_atom(false, b"WM_DELETE_WINDOW")?
            .reply()
            .context("Failed to intern WM_DELETE_WINDOW")?
            .atom;

        let wm_state = conn
            .intern_atom(false, b"WM_STATE")?
            .reply()
            .context("Failed to intern WM_STATE")?
            .atom;

        let wm_take_focus = conn
            .intern_atom(false, b"WM_TAKE_FOCUS")?
            .reply()
            .context("Failed to intern WM_TAKE_FOCUS")?
            .atom;

        let net_wm_state = conn
            .intern_atom(false, b"_NET_WM_STATE")?
            .reply()
            .context("Failed to intern _NET_WM_STATE")?
            .atom;

        let net_wm_state_fullscreen = conn
            .intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")?
            .reply()
            .context("Failed to intern _NET_WM_STATE_FULLSCREEN")?
            .atom;

        let net_active_window = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()
            .context("Failed to intern _NET_ACTIVE_WINDOW")?
            .atom;

        Ok(Self {
            wm_protocols,
            wm_delete_window,
            wm_state,
            wm_take_focus,
            net_wm_state,
            net_wm_state_fullscreen,
            net_active_window,
        })
    }
}
