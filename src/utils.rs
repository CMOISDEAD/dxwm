use std::collections::HashSet;
use std::env::home_dir;
use std::str;

use std::io::Read;
use std::process::{Command, Stdio};

pub const XK_A: u32 = 0x0061;
pub const XK_B: u32 = 0x0062;
pub const XK_C: u32 = 0x0063;
pub const XK_D: u32 = 0x0064;
pub const XK_E: u32 = 0x0065;
pub const XK_F: u32 = 0x0066;
pub const XK_G: u32 = 0x0067;
pub const XK_H: u32 = 0x0068;
pub const XK_I: u32 = 0x0069;
pub const XK_J: u32 = 0x006a;
pub const XK_K: u32 = 0x006b;
pub const XK_L: u32 = 0x006c;
pub const XK_M: u32 = 0x006d;
pub const XK_N: u32 = 0x006e;
pub const XK_O: u32 = 0x006f;
pub const XK_P: u32 = 0x0070;
pub const XK_Q: u32 = 0x0071;
pub const XK_R: u32 = 0x0072;
pub const XK_S: u32 = 0x0073;
pub const XK_T: u32 = 0x0074;
pub const XK_U: u32 = 0x0075;
pub const XK_V: u32 = 0x0076;
pub const XK_W: u32 = 0x0077;
pub const XK_X: u32 = 0x0078;
pub const XK_Y: u32 = 0x0079;
pub const XK_Z: u32 = 0x007a;

pub const XK_SPACE: u32 = 0x0020;
pub const XK_EXCLAM: u32 = 0x0021;
pub const XK_QUOTEDBL: u32 = 0x0022;
pub const XK_NUMBERSIGN: u32 = 0x0023;
pub const XK_DOLLAR: u32 = 0x0024;
pub const XK_PERCENT: u32 = 0x0025;
pub const XK_AMPERSAND: u32 = 0x0026;
pub const XK_APOSTROPHE: u32 = 0x0027;
pub const XK_PARENLEFT: u32 = 0x0028;
pub const XK_PARENRIGHT: u32 = 0x0029;
pub const XK_ASTERISK: u32 = 0x002a;
pub const XK_PLUS: u32 = 0x002b;
pub const XK_COMMA: u32 = 0x002c;
pub const XK_MINUS: u32 = 0x002d;
pub const XK_PERIOD: u32 = 0x002e;
pub const XK_SLASH: u32 = 0x002f;
pub const XK_COLON: u32 = 0x003a;
pub const XK_SEMICOLON: u32 = 0x003b;
pub const XK_LESS: u32 = 0x003c;
pub const XK_EQUAL: u32 = 0x003d;
pub const XK_GREATER: u32 = 0x003e;
pub const XK_QUESTION: u32 = 0x003f;
pub const XK_AT: u32 = 0x0040;
pub const XK_BRACKETLEFT: u32 = 0x005b;
pub const XK_BACKSLASH: u32 = 0x005c;
pub const XK_BRACKETRIGHT: u32 = 0x005d;
pub const XK_ASCIICIRCUM: u32 = 0x005e;
pub const XK_UNDERSCORE: u32 = 0x005f;
pub const XK_BRACELEFT: u32 = 0x007b;
pub const XK_BAR: u32 = 0x007c;
pub const XK_BRACERIGHT: u32 = 0x007d;
pub const XK_ASCIITILDE: u32 = 0x007e;

pub const XK_BACKSPACE: u32 = 0xff08;
pub const XK_TAB: u32 = 0xff09;
pub const XK_RETURN: u32 = 0xff0d;
pub const XK_ESCAPE: u32 = 0xff1b;
pub const XK_DELETE: u32 = 0xffff;
pub const XK_HOME: u32 = 0xff50;
pub const XK_LEFT: u32 = 0xff51;
pub const XK_UP: u32 = 0xff52;
pub const XK_RIGHT: u32 = 0xff53;
pub const XK_DOWN: u32 = 0xff54;
pub const XK_PAGE_UP: u32 = 0xff55;
pub const XK_PAGE_DOWN: u32 = 0xff56;
pub const XK_END: u32 = 0xff57;
pub const XK_INSERT: u32 = 0xff63;
pub const XK_PRINT: u32 = 0xff61;
pub const XK_GRAVE: u32 = 0x0060;

pub const XK_SHIFT_L: u32 = 0xffe1;
pub const XK_SHIFT_R: u32 = 0xffe2;
pub const XK_CONTROL_L: u32 = 0xffe3;
pub const XK_CONTROL_R: u32 = 0xffe4;
pub const XK_CAPS_LOCK: u32 = 0xffe5;
pub const XK_META_L: u32 = 0xffe7;
pub const XK_META_R: u32 = 0xffe8;
pub const XK_ALT_L: u32 = 0xffe9;
pub const XK_ALT_R: u32 = 0xffea;
pub const XK_SUPER_L: u32 = 0xffeb;
pub const XK_SUPER_R: u32 = 0xffec;

pub const XK_F1: u32 = 0xffbe;
pub const XK_F2: u32 = 0xffbf;
pub const XK_F3: u32 = 0xffc0;
pub const XK_F4: u32 = 0xffc1;
pub const XK_F5: u32 = 0xffc2;
pub const XK_F6: u32 = 0xffc3;
pub const XK_F7: u32 = 0xffc4;
pub const XK_F8: u32 = 0xffc5;
pub const XK_F9: u32 = 0xffc6;
pub const XK_F10: u32 = 0xffc7;
pub const XK_F11: u32 = 0xffc8;
pub const XK_F12: u32 = 0xffc9;

pub const XK_0: u32 = 0x0030;
pub const XK_1: u32 = 0x0031;
pub const XK_2: u32 = 0x0032;
pub const XK_3: u32 = 0x0033;
pub const XK_4: u32 = 0x0034;
pub const XK_5: u32 = 0x0035;
pub const XK_6: u32 = 0x0036;
pub const XK_7: u32 = 0x0037;
pub const XK_8: u32 = 0x0038;
pub const XK_9: u32 = 0x0039;

pub const XK_AUDIO_LOWER_VOL: u32 = 0x1008ff11;
pub const XK_AUDIO_MUTE: u32 = 0x1008ff12;
pub const XK_AUDIO_RAISE_VOL: u32 = 0x1008ff13;
pub const XK_AUDIO_PLAY: u32 = 0x1008ff14;
pub const XK_AUDIO_STOP: u32 = 0x1008ff15;
pub const XK_AUDIO_PREV: u32 = 0x1008ff16;
pub const XK_AUDIO_NEXT: u32 = 0x1008ff17;
pub const XK_AUDIO_MIC_MUTE: u32 = 0x1008ffb2;

pub const XK_MON_BRIGHTNESS_UP: u32 = 0x1008ff02;
pub const XK_MON_BRIGHTNESS_DOWN: u32 = 0x1008ff03;

pub const XK_CALCULATOR: u32 = 0x1008ff1d;
pub const XK_MAIL: u32 = 0x1008ff19;
pub const XK_WWW: u32 = 0x1008ff2e;
pub const XK_TOOLS: u32 = 0x1008ff81;

pub fn run_autostart() {
    let mut path = home_dir().expect("No se pudo encontrar el directorio home");
    path.push(".local/share/dxwm/autostart.sh");

    if path.exists() {
        Command::new("sh")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Fallo al lanzar el script de autostart");

        println!("Autostart ejecutado con éxito.");
    } else {
        eprintln!("No se encontró el archivo autostart en: {:?}", path);
    }
}

pub fn get_command_output(cmd: &str) -> String {
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return format!("FAIL_SPAWN: {}", e),
    };

    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        if let Err(e) = stdout.read_to_string(&mut output) {
            return format!("FAIL_READ: {}", e);
        }
    }

    match child.wait() {
        Ok(_) => {}
        Err(e) => {
            if let Some(code) = e.raw_os_error() {
                if code == 10 {
                } else {
                    println!("[WARN] Error waiting the process: {}", e);
                }
            }
        }
    }

    output.trim().to_string()
}

pub fn get_volume() -> String {
    get_command_output("pamixer --get-volume")
        .trim()
        .to_string()
}

pub fn is_muted() -> bool {
    get_command_output("pamixer --get-mute").trim() == "true"
}

pub fn dedup_preserve_order<T>(vec: Vec<T>) -> Vec<T>
where
    T: std::hash::Hash + Eq + Clone,
{
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for item in vec {
        if seen.insert(item.clone()) {
            result.push(item);
        }
    }

    result
}
