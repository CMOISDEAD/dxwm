use std::process::Command;

pub const BORDER_WIDTH: u32 = 1;
pub const MARGIN: u32 = 5;

pub const BACKGROUND: u32 = 0x1D2021;
pub const FOREGROUND: u32 = 0xFBFBFB;
pub const BORDER_FOCUSED: u32 = 0xCCCCCC;
pub const BORDER_UNFOCUSED: u32 = 0x5C5C5C;
pub const SELECTED: u32 = 0x5C5C5C;
pub const FONT_NAME: &str = "Terminess Nerd Font";

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

/*

 ((bg
   (cond ((equal doom-gruvbox-dark-variant "hard") '("#1d2021" "#1e1e1e" nil))   ; bg0_h
         ((equal doom-gruvbox-dark-variant "soft") '("#32302f" "#323232" nil))   ; bg0_s
         (t                                        '("#282828" "#282828" nil)))) ; bg0
  (bg-alt
   (cond ((equal doom-gruvbox-dark-variant "hard") '("#0d1011" "black" nil))     ; (self-defined)
         ((equal doom-gruvbox-dark-variant "soft") '("#282828" "#282828" nil))   ; bg0
         (t                                        '("#1d2021" "#1e1e1e" nil)))) ; bg_h
  (bg-alt2    '("#504945" "#504945" "brown"      )) ; bg2 (for region, selection etc.)

  (base0      '("#0d1011" "black"   "black"      )) ; (self-defined)
  (base1      '("#1d2021" "#1d1d1d" "brightblack")) ; bg0_h
  (base2      '("#282828" "#282828" "brightblack")) ; bg0
  (base3      '("#3c3836" "#383838" "brightblack")) ; bg1
  (base4      '("#665c54" "#5c5c5c" "brightblack")) ; bg3
  (base5      '("#7c6f64" "#6f6f6f" "brightblack")) ; bg4
  (base6      '("#928374" "#909090" "brightblack")) ; gray
  (base7      '("#d5c4a1" "#cccccc" "brightblack")) ; fg2
  (base8      '("#fbf1c7" "#fbfbfb" "brightwhite")) ; fg0
  (fg         '("#ebdbb2" "#dfdfdf" "brightwhite")) ; fg/fg1
  (fg-alt     '("#d5c4a1" "#cccccc" "brightwhite")) ; fg2

*/
