pub mod keybinds;

use std::process::Command;

pub const NUM_WORKSPACES: u8 = 9;

pub const BORDER_WIDTH: u32 = 15;
/// Space between the clients and the monitor edges, also used to place the alerts
pub const MARGIN: u32 = 10;

pub const BACKGROUND: u32 = 0xD7D5D1;
pub const FOREGROUND: u32 = 0x222222;
pub const BORDER_FOCUSED: u32 = 0xB8B6B2;
pub const BORDER_UNFOCUSED: u32 = 0x5C5C5C;
pub const SELECTED: u32 = 0x222222;
pub const FONT_NAME: &str = "GoMono Nerd Font Mono Regular";

pub const TERMINAL_APP: &str = "alacritty";
pub const FILEMANAGER_APP: &str = "pcmanfm";
pub const EDITOR_APP: &str = "emacsclient -c";

pub fn launch_dmenu() {
    Command::new("dmenu_run")
        .arg("-fn")
        .arg(format!("{}:size=9", FONT_NAME))
        .arg("-nb")
        .arg(format!("#{:06x}", BACKGROUND))
        .arg("-nf")
        .arg(format!("#{:06x}", FOREGROUND))
        .arg("-sb")
        .arg(format!("#{:06x}", SELECTED))
        .spawn()
        .ok();
}
