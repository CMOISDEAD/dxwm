use anyhow::{Context, Result};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

pub struct Keymap {
    keycodes: HashMap<u32, Keycode>,
    /// Keysyms of every keycode, `per_keycode` each, starting at `min_keycode`
    keysyms: Vec<u32>,
    per_keycode: usize,
    min_keycode: Keycode,
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

        Ok(Self {
            keycodes,
            keysyms: mapping.keysyms,
            per_keycode,
            min_keycode: setup.min_keycode,
        })
    }

    pub fn keycode(&self, keysym: u32) -> Option<Keycode> {
        self.keycodes.get(&keysym).copied()
    }

    /// Keysym produced by a key with the modifiers of a key event (0 if none):
    /// Shift picks the second level and AltGr the third and fourth
    pub fn keysym(&self, keycode: Keycode, state: u16) -> u32 {
        let row = keycode
            .checked_sub(self.min_keycode)
            .and_then(|i| self.keysyms.chunks(self.per_keycode).nth(i as usize))
            .unwrap_or_default();
        let level = |column: usize| row.get(column).copied().filter(|&k| k != 0);

        let shift = usize::from(state & u16::from(ModMask::SHIFT) != 0);
        // With XKB the levels reached with AltGr are the columns 4 and 5
        let base = if state & u16::from(ModMask::M5) != 0 {
            4
        } else {
            0
        };

        level(base + shift)
            .or_else(|| level(base))
            .or_else(|| level(shift))
            .or_else(|| level(0))
            .unwrap_or(0)
    }
}

pub fn keysym_to_char(keysym: u32) -> Option<char> {
    match keysym {
        // Latin-1 keysyms have the value of their character
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(keysym),
        // Any other character is its Unicode code point + 0x01000000
        0x0100_0100..=0x0110_ffff => char::from_u32(keysym - 0x0100_0000),
        _ => None,
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

pub fn normalize_modifiers(modifiers: ModMask) -> ModMask {
    let ignored = u16::from(ModMask::M2 | ModMask::LOCK);
    ModMask::from(u16::from(modifiers) & 0xff & !ignored)
}
