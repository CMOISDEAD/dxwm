pub mod keybinds;

use std::process::Command;

pub const NUM_WORKSPACES: u8 = 9;

pub const BORDER_WIDTH: u32 = 3;
/// Space between the clients and the monitor edges, also used to place the alerts
pub const MARGIN: u32 = 10;

/// Width of a new tiled client as a fraction of the monitor, 0.5 fits two side by side
pub const DEFAULT_COLUMN_WIDTH: f32 = 0.5;
/// Widths cycled with Super+R
pub const COLUMN_WIDTH_PRESETS: [f32; 3] = [1.0 / 3.0, 0.5, 2.0 / 3.0];
/// Change applied by Super+H / Super+L
pub const COLUMN_WIDTH_STEP: f32 = 0.1;
pub const MIN_COLUMN_WIDTH: f32 = 0.2;

pub const BACKGROUND: u32 = 0xD7D5D1;
pub const FOREGROUND: u32 = 0x222222;
pub const BORDER_FOCUSED: u32 = 0xB8B6B2;
pub const BORDER_UNFOCUSED: u32 = 0x5C5C5C;
pub const SELECTED: u32 = 0x222222;
pub const FONT_NAME: &str = "GoMono Nerd Font Mono Regular";

/// Title bar on top of each client, inside the border. 0 disables it
pub const TITLE_HEIGHT: u32 = 20;
pub const TITLE_BG_FOCUSED: u32 = BORDER_FOCUSED;
pub const TITLE_BG_UNFOCUSED: u32 = BORDER_UNFOCUSED;
pub const TITLE_FG_FOCUSED: u32 = 0x222222;
pub const TITLE_FG_UNFOCUSED: u32 = 0xD7D5D1;
/// Space between the left edge of the title bar and the text
pub const TITLE_PADDING: i16 = 6;
/// X core font (XLFD) for the titles, "fixed" is used if it can't be opened.
/// An iso10646 font is needed to show non latin-1 characters
pub const TITLE_FONT: &str = "-misc-fixed-medium-r-normal--13-*-*-*-*-*-iso10646-1";

/// Where screenshots are saved, expanded by the shell
pub const SCREENSHOT_DIR: &str = "$HOME/Pictures/Screenshots";

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
