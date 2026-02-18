use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

#[allow(dead_code)]
pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete_window: Atom,
    pub wm_state: Atom,
    pub wm_take_focus: Atom,
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

        Ok(Self {
            wm_protocols,
            wm_delete_window,
            wm_state,
            wm_take_focus,
        })
    }
}
