use std::collections::HashMap;
use x11rb::protocol::xproto::{Keycode, ModMask};

use crate::wm::WindowManager;

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
    Custom(fn(&mut WindowManager)),
}

#[derive(Clone, Debug)]
pub struct KeyBinding {
    pub keycode: Keycode,
    pub modifiers: ModMask,
    pub action: KeyAction,
}

/// A named set of bindings without modifiers, entered from normal mode
#[derive(Clone, Debug)]
pub struct SubMap {
    pub bindings: Vec<KeyBinding>,
    /// Go back to normal mode after running one action
    pub oneshot: bool,
}

#[derive(Default)]
pub struct KeyBindingManager {
    pub normal_bindings: Vec<KeyBinding>,
    pub submaps: HashMap<String, SubMap>,
    pub current_mode: Option<String>,
}

impl KeyBindingManager {
    pub fn bind_normal(&mut self, keycode: Keycode, modifiers: ModMask, action: KeyAction) {
        self.normal_bindings.push(KeyBinding {
            keycode,
            modifiers,
            action,
        });
    }

    pub fn add_submap(&mut self, name: &str, oneshot: bool) {
        self.submaps.insert(
            name.to_string(),
            SubMap {
                bindings: Vec::new(),
                oneshot,
            },
        );
    }

    pub fn bind_in_mode(&mut self, mode: &str, keycode: Keycode, action: KeyAction) {
        if let Some(submap) = self.submaps.get_mut(mode) {
            submap.bindings.push(KeyBinding {
                keycode,
                modifiers: ModMask::default(),
                action,
            });
        }
    }

    fn current_submap(&self) -> Option<&SubMap> {
        self.current_mode
            .as_ref()
            .and_then(|mode| self.submaps.get(mode))
    }

    /// Bindings of the current mode
    pub fn active_bindings(&self) -> &[KeyBinding] {
        match self.current_submap() {
            Some(submap) => &submap.bindings,
            None => &self.normal_bindings,
        }
    }

    pub fn find_action(&self, keycode: Keycode, modifiers: ModMask) -> Option<KeyAction> {
        self.active_bindings()
            .iter()
            .find(|b| b.keycode == keycode && b.modifiers == modifiers)
            .map(|b| b.action.clone())
    }

    pub fn enter_mode(&mut self, mode: String) {
        if self.submaps.contains_key(&mode) {
            println!("Entering mode: {}", mode);
            self.current_mode = Some(mode);
        }
    }

    pub fn exit_mode(&mut self) {
        if let Some(mode) = self.current_mode.take() {
            println!("Exiting mode: {}", mode);
        }
    }

    /// Whether the current submap goes back to normal mode after an action
    pub fn should_auto_exit(&self) -> bool {
        self.current_submap().is_some_and(|s| s.oneshot)
    }

    pub fn is_in_submap(&self) -> bool {
        self.current_mode.is_some()
    }
}
