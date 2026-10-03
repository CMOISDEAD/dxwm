use std::process::{Child, Command, Stdio};

use anyhow::Result;

use crate::config::SCREENSHOT_DIR;
use crate::wm::WindowManager;

#[derive(Clone, Copy)]
pub enum Shot {
    /// The current monitor
    Monitor,
    /// An area or window selected with the mouse (Escape cancels)
    Area,
    /// The focused client, with its decoration
    Window,
}

/// A screenshot being taken by a background process
pub struct Screenshot {
    child: Child,
    label: &'static str,
}

impl WindowManager {
    /// Save a screenshot (maim) to `SCREENSHOT_DIR` and copy it to the clipboard
    /// (xclip). It runs in the background: the WM must never wait for it, xclip
    /// stays alive to serve the clipboard and an area selection can take long
    pub fn screenshot(&mut self, shot: Shot) -> Result<()> {
        if self.screenshot.is_some() {
            return Ok(());
        }

        let (label, target) = match shot {
            Shot::Monitor => {
                let m = self.monitors.current();
                let geometry = format!("-g {}x{}+{}+{}", m.width, m.height, m.x, m.y);
                ("MONITOR", geometry)
            }
            Shot::Area => ("AREA", "-s".to_string()),
            Shot::Window => {
                let Some(client) = self.focused_client().and_then(|w| self.client(w)) else {
                    return Ok(());
                };
                ("WINDOW", format!("-i {}", client.frame))
            }
        };

        // -u hides the pointer
        let script = format!(
            r#"dir="{SCREENSHOT_DIR}"; mkdir -p "$dir" &&
            file="$dir/$(date +%Y-%m-%d_%H-%M-%S).png" &&
            maim -u {target} "$file" &&
            xclip -selection clipboard -t image/png -i "$file""#
        );

        // Alerts shouldn't appear in the picture
        self.clear_alerts()?;

        let child = Command::new("sh")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        self.screenshot = Some(Screenshot { child, label });
        Ok(())
    }

    /// Called from the main loop: show an alert once the screenshot is done
    pub fn poll_screenshot(&mut self) -> Result<()> {
        let Some(screenshot) = &mut self.screenshot else {
            return Ok(());
        };

        let success = match screenshot.child.try_wait() {
            Ok(None) => return Ok(()),
            Ok(Some(status)) => status.success(),
            Err(_) => false,
        };

        let message = match (success, screenshot.label) {
            (true, label) => format!("[SCR] {}", label),
            (false, "AREA") => "[SCR] CANCELLED".to_string(),
            (false, _) => "[SCR] FAILED".to_string(),
        };

        self.screenshot = None;
        self.draw_alert(message)
    }
}
