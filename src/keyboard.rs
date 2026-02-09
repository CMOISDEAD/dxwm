use anyhow::{Context, Result};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

pub struct KeySymConverter {
    keysym_map: HashMap<u32, Vec<Keycode>>,
}

impl KeySymConverter {
    pub fn new<C: Connection>(conn: &C, setup: &Setup) -> Result<Self> {
        let mut keysym_map = HashMap::new();

        let keyboard_mapping = conn
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()
            .context("Failed to get keyboard mapping")?;

        let keysyms_per_keycode = keyboard_mapping.keysyms_per_keycode as usize;

        for keycode in setup.min_keycode..=setup.max_keycode {
            let index = (keycode - setup.min_keycode) as usize;
            let offset = index * keysyms_per_keycode;

            if offset + keysyms_per_keycode <= keyboard_mapping.keysyms.len() {
                for i in 0..keysyms_per_keycode {
                    let keysym = keyboard_mapping.keysyms[offset + i];
                    if keysym != 0 {
                        keysym_map
                            .entry(keysym)
                            .or_insert_with(Vec::new)
                            .push(keycode);
                    }
                }
            }
        }

        Ok(Self { keysym_map })
    }

    pub fn keysym_to_keycode(&self, keysym: u32) -> Option<Keycode> {
        self.keysym_map
            .get(&keysym)
            .and_then(|codes| codes.first().copied())
    }
}

pub struct KeyboardGrabber<'a, C: Connection> {
    conn: &'a C,
    root: Window,
    converter: KeySymConverter,
}

impl<'a, C: Connection> KeyboardGrabber<'a, C> {
    pub fn new(conn: &'a C, screen: &Screen, setup: &Setup) -> Result<Self> {
        let converter = KeySymConverter::new(conn, setup)?;

        Ok(Self {
            conn,
            root: screen.root,
            converter,
        })
    }

    pub fn grab_key(&self, keycode: Keycode, modifiers: ModMask) -> Result<()> {
        self.conn
            .grab_key(
                false,
                self.root,
                modifiers,
                keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()
            .context("Failed to grab key")?;

        // También grab con NumLock activado
        if !modifiers.contains(ModMask::M2) {
            self.conn
                .grab_key(
                    false,
                    self.root,
                    modifiers | ModMask::M2,
                    keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )?
                .check()
                .ok();
        }

        Ok(())
    }

    /// Libera todos los grabs
    pub fn ungrab_all_keys(&self) -> Result<()> {
        self.conn
            .ungrab_key(0, self.root, ModMask::ANY)?
            .check()
            .context("Failed to ungrab all keys")
    }

    pub fn keysym_to_keycode(&self, keysym: u32) -> Option<Keycode> {
        self.converter.keysym_to_keycode(keysym)
    }
}

/// Normaliza los modifiers ignorando NumLock y CapsLock
pub fn normalize_modifiers(modifiers: ModMask) -> ModMask {
    modifiers // FIXME:& !(ModMask::M2 | ModMask::LOCK)
}
