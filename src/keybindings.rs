use std::collections::HashMap;
use x11rb::protocol::xproto::{Keycode, ModMask};

#[derive(Debug, Clone)]
pub enum KeyAction {
    Spawn(String),
    EnterMode(String),
    ExitMode,
    CloseWindow,
    FocusNext,
    FocusPrev,
    Quit,
    SwitchWorkspace(u8),
    MoveToWorkspace(u8),
    Custom(fn(&mut crate::wm::WindowManager)),
}

#[derive(Clone, Debug)]
pub struct KeyBinding {
    pub keycode: Keycode,
    pub modifiers: ModMask,
    pub action: KeyAction,
}

#[derive(Clone, Debug)]
pub struct SubMap {
    pub name: String,
    pub bindings: Vec<KeyBinding>,
    pub oneshot: bool,
}

pub struct KeyBindingManager {
    pub normal_bindings: Vec<KeyBinding>,
    pub submaps: HashMap<String, SubMap>,
    pub current_mode: Option<String>,
}

impl KeyBindingManager {
    pub fn new() -> Self {
        Self {
            normal_bindings: Vec::new(),
            submaps: HashMap::new(),
            current_mode: None,
        }
    }

    pub fn bind_normal(&mut self, keycode: Keycode, modifiers: ModMask, action: KeyAction) {
        self.normal_bindings.push(KeyBinding {
            keycode,
            modifiers,
            action,
        });
    }

    pub fn add_submap(&mut self, name: String, oneshot: bool) {
        self.submaps.insert(
            name.clone(),
            SubMap {
                name,
                bindings: Vec::new(),
                oneshot,
            },
        );
    }

    pub fn bind_in_mode(
        &mut self,
        mode: &str,
        keycode: Keycode,
        modifiers: ModMask,
        action: KeyAction,
    ) {
        if let Some(submap) = self.submaps.get_mut(mode) {
            submap.bindings.push(KeyBinding {
                keycode,
                modifiers,
                action,
            });
        }
    }

    pub fn active_bindings(&self) -> &[KeyBinding] {
        if let Some(mode_name) = &self.current_mode {
            if let Some(submap) = self.submaps.get(mode_name) {
                return &submap.bindings;
            }
        }
        &self.normal_bindings
    }

    pub fn find_action(&self, keycode: Keycode, modifiers: ModMask) -> Option<KeyAction> {
        let bindings = self.active_bindings();

        for binding in bindings {
            if binding.keycode != keycode {
                continue;
            }

            if binding.modifiers == ModMask::default() {
                if modifiers == ModMask::default() {
                    return Some(binding.action.clone());
                }
            } else {
                if binding.modifiers == modifiers {
                    return Some(binding.action.clone());
                }
            }
        }

        None
    }

    pub fn enter_mode(&mut self, mode: String) {
        if self.submaps.contains_key(&mode) {
            println!("→ Entering mode: {}", mode);
            self.current_mode = Some(mode);
        }
    }

    pub fn exit_mode(&mut self) {
        if let Some(mode) = &self.current_mode {
            println!("← Exiting mode: {}", mode);
            self.current_mode = None;
        }
    }

    pub fn should_auto_exit(&self) -> bool {
        if let Some(mode_name) = &self.current_mode {
            if let Some(submap) = self.submaps.get(mode_name) {
                return submap.oneshot;
            }
        }
        false
    }

    pub fn is_normal_mode(&self) -> bool {
        self.current_mode.is_none()
    }

    pub fn is_in_submap(&self) -> bool {
        self.current_mode.is_some()
    }
}
