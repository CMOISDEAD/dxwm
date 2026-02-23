use std::process::Command;

pub const BORDER_WIDTH: u32 = 1;
pub const MARGIN: u32 = 5;

pub const BACKGROUND: u32 = 0x222222;
pub const FOREGROUND: u32 = 0xD7D5D1;
pub const BORDER_FOCUSED: u32 = 0xBBBBBB;
pub const BORDER_UNFOCUSED: u32 = 0x444444;
pub const FONT_NAME: &str = "Terminess Nerd Font";
pub const SELECTED: u32 = 0x444444;

pub const TERMINAL_APP: &str = "alacritty";
pub const FILEMANAGER_APP: &str = "pcmanfm";
pub const EDITOR_APP: &str = "emacs";
pub const BROWSER_APP: &str = "qutebrowser";

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
