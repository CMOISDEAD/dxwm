use anyhow::Result;
use x11rb::properties::WmClass;
use x11rb::protocol::xproto::Window;

use crate::config::{RULES, Rule};
use crate::wm::WindowManager;

impl WindowManager {
    pub fn rule_for(&self, window: Window) -> Result<Option<&'static Rule>> {
        let Some(class) = WmClass::get(&self.conn, window)?.reply().ok().flatten() else {
            return Ok(None);
        };

        Ok(RULES.iter().find(|rule| {
            [class.instance(), class.class()]
                .iter()
                .any(|name| name.eq_ignore_ascii_case(rule.class.as_bytes()))
        }))
    }
}
