pub mod keybinds;

pub const NUM_WORKSPACES: usize = 9;

pub const BORDER_WIDTH: u32 = 1;
pub const MARGIN: u32 = 10;

/// Fractions of the monitor width, 0.5 fits two columns side by side
pub const DEFAULT_COLUMN_WIDTH: f32 = 0.5;
pub const COLUMN_WIDTH_PRESETS: [f32; 3] = [1.0 / 3.0, 0.5, 2.0 / 3.0];
pub const COLUMN_WIDTH_STEP: f32 = 0.1;
pub const MIN_COLUMN_WIDTH: f32 = 0.2;
pub const CENTER_FOCUSED_COLUMN: bool = false;

pub const BACKGROUND: u32 = 0xD7D5D1;
pub const FOREGROUND: u32 = 0x222222;
pub const BORDER_FOCUSED: u32 = 0xB8B6B2;
pub const BORDER_UNFOCUSED: u32 = 0x5C5C5C;
pub const SELECTED_BG: u32 = 0x222222;
pub const SELECTED_FG: u32 = 0xD7D5D1;
pub const DIMMED_FG: u32 = 0x77756F;

/// 0 disables the title bar
pub const TITLE_HEIGHT: u32 = 20;
pub const TITLE_BG_FOCUSED: u32 = BORDER_FOCUSED;
pub const TITLE_BG_UNFOCUSED: u32 = BORDER_UNFOCUSED;
pub const TITLE_FG_FOCUSED: u32 = 0x222222;
pub const TITLE_FG_UNFOCUSED: u32 = 0xD7D5D1;
pub const BORDER_URGENT: u32 = SELECTED_BG;
pub const TITLE_BG_URGENT: u32 = SELECTED_BG;
pub const TITLE_FG_URGENT: u32 = SELECTED_FG;
pub const TITLE_PADDING: i16 = 6;
/// X core font (XLFD), "fixed" if it can't be opened. Only iso10646 fonts show
/// non latin-1 characters
pub const TITLE_FONT: &str = "-misc-fixed-medium-r-normal--13-*-*-*-*-*-iso10646-1";

/// 580 fits 80 columns of TITLE_FONT
pub const PROMPT_WIDTH: u16 = 580;
pub const PROMPT_LINES: usize = 10;
pub const PROMPT_OUTPUT_LINES: usize = 30;
pub const PROMPT_HISTORY: usize = 100;

pub const ALERT_LINES: usize = 4;
pub const ALERT_HISTORY: u64 = 60;

/// Modeline drawn under every window, only seen where the desktop is
pub const SHOW_BAR: bool = false;
/// Pixels a whole monitor takes in the minimap of the workspaces
pub const MINIMAP_VIEW: i32 = 24;

/// Expanded by the shell
pub const SCREENSHOT_DIR: &str = "$HOME/Pictures/Screenshots";

pub const TERMINAL_APP: &str = "alacritty";
pub const FILEMANAGER_APP: &str = "pcmanfm";
pub const EDITOR_APP: &str = "emacsclient -c";

pub struct Rule {
    pub class: &'static str,
    pub workspace: Option<usize>,
    pub floating: Option<bool>,
    pub column_width: Option<f32>,
}

impl Rule {
    pub const fn class(class: &'static str) -> Self {
        Self {
            class,
            workspace: None,
            floating: None,
            column_width: None,
        }
    }
}

/// `class` is matched against both parts of `WM_CLASS` ignoring case, `workspace`
/// is the number shown to the user
pub const RULES: &[Rule] = &[Rule {
    floating: Some(true),
    ..Rule::class("pavucontrol")
}];
