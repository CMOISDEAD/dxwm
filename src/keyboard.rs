use anyhow::{Context, Result};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

/// Keysym -> keycode table of the current keyboard layout
pub struct Keymap {
    keycodes: HashMap<u32, Keycode>,
}

impl Keymap {
    pub fn new<C: Connection>(conn: &C) -> Result<Self> {
        let setup = conn.setup();
        let count = setup.max_keycode - setup.min_keycode + 1;

        let mapping = conn
            .get_keyboard_mapping(setup.min_keycode, count)?
            .reply()
            .context("Failed to get keyboard mapping")?;

        let per_keycode = mapping.keysyms_per_keycode as usize;
        let mut keycodes = HashMap::new();

        for (i, keysyms) in mapping.keysyms.chunks(per_keycode).enumerate() {
            let keycode = setup.min_keycode + i as u8;

            for &keysym in keysyms.iter().filter(|&&k| k != 0) {
                // Keep the lowest keycode producing the keysym
                keycodes.entry(keysym).or_insert(keycode);
            }
        }

        Ok(Self { keycodes })
    }

    pub fn keycode(&self, keysym: u32) -> Option<Keycode> {
        self.keycodes.get(&keysym).copied()
    }
}

/// NumLock/CapsLock combinations a grab has to be repeated with, so they don't
/// break the bindings
pub fn lock_masks() -> [ModMask; 4] {
    [
        ModMask::default(),
        ModMask::M2,
        ModMask::LOCK,
        ModMask::M2 | ModMask::LOCK,
    ]
}

/// Grab a key combination, also with NumLock and CapsLock active
/// (`normalize_modifiers` ignores them on key press)
pub fn grab_key<C: Connection>(
    conn: &C,
    root: Window,
    keycode: Keycode,
    modifiers: ModMask,
) -> Result<()> {
    for extra in lock_masks() {
        conn.grab_key(
            false,
            root,
            modifiers | extra,
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?;
    }

    Ok(())
}

/// Grab the whole keyboard, used while a submap is active so any key can exit it
pub fn grab_keyboard<C: Connection>(conn: &C, root: Window) -> Result<()> {
    conn.grab_key(
        false,
        root,
        ModMask::ANY,
        Grab::ANY,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?;
    Ok(())
}

pub fn ungrab_all_keys<C: Connection>(conn: &C, root: Window) -> Result<()> {
    conn.ungrab_key(Grab::ANY, root, ModMask::ANY)?;
    Ok(())
}

/// Keep only the modifier bits, ignoring NumLock and CapsLock
pub fn normalize_modifiers(modifiers: ModMask) -> ModMask {
    let ignored = u16::from(ModMask::M2 | ModMask::LOCK);
    ModMask::from(u16::from(modifiers) & 0xff & !ignored)
}
