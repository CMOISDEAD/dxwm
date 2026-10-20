mod alerts;
mod atoms;
mod bar;
mod clients;
mod config;
mod decorations;
mod ewmh;
mod floating;
mod keybindings;
mod keyboard;
mod keysyms;
mod layout;
mod monitors;
mod prompt;
mod restart;
mod screenshot;
mod utils;
mod wm;
mod workspaces;

use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::Result;

use crate::utils::run_autostart;

const RESTART_FLAG: &str = "--restart";

fn main() -> Result<()> {
    let restarted = env::args().skip(1).any(|arg| arg == RESTART_FLAG);

    let mut wm = wm::WindowManager::new(restarted)?;
    wm.setup_keybindings()?;
    wm.adopt_windows()?;

    if !restarted {
        run_autostart();
    }

    let exit = wm.run()?;
    drop(wm);
    if exit == wm::Exit::Quit {
        return Ok(());
    }

    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| "dxwm".into());
    let args: Vec<_> = args.filter(|arg| arg != RESTART_FLAG).collect();

    // argv[0] and not current_exe(), so a rebuilt binary is the one that runs
    Err(Command::new(program)
        .args(args)
        .arg(RESTART_FLAG)
        .exec()
        .into())
}
