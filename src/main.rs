mod alerts;
mod atoms;
mod clients;
mod config;
mod decorations;
mod floating;
mod keybindings;
mod keyboard;
mod keysyms;
mod layout;
mod monitors;
mod prompt;
mod screenshot;
mod utils;
mod wm;
mod workspaces;

use anyhow::Result;

fn main() -> Result<()> {
    let mut wm = wm::WindowManager::new()?;
    wm.setup_keybindings()?;
    wm.run()
}
