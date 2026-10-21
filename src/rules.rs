use anyhow::Result;
use x11rb::properties::WmClass;
use x11rb::protocol::xproto::Window;

use crate::config::{RULES, Rule};
use crate::wm::WindowManager;

fn matches(class: &WmClass, name: &str) -> bool {
    [class.instance(), class.class()]
        .iter()
        .any(|part| part.eq_ignore_ascii_case(name.as_bytes()))
}

impl WindowManager {
    fn read_class(&self, window: Window) -> Result<Option<WmClass>> {
        Ok(WmClass::get(&self.conn, window)?.reply().ok().flatten())
    }

    pub fn rule_for(&self, window: Window) -> Result<Option<&'static Rule>> {
        let Some(class) = self.read_class(window)? else {
            return Ok(None);
        };
        Ok(RULES.iter().find(|rule| matches(&class, rule.class)))
    }

    pub fn has_class(&self, window: Window, name: &str) -> Result<bool> {
        Ok(self
            .read_class(window)?
            .is_some_and(|class| matches(&class, name)))
    }
}
