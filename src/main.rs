mod alerts;
mod clients;
mod config;
mod keybindings;
mod keyboard;
mod layout;
mod utils;
mod wm;
mod workspaces;

use anyhow::Result;

fn main() -> Result<()> {
    let mut wm = wm::WindowManager::new()?;

    wm.setup_keybindings()?;

    wm.run()?;

    Ok(())
}
