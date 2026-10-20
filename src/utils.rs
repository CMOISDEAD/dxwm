use std::env::home_dir;
use std::fs;
use std::process::{Command, Stdio};

pub fn run_autostart() {
    let Some(mut path) = home_dir() else {
        eprintln!("Home directory not found, skipping autostart");
        return;
    };
    path.push(".local/share/dxwm/autostart.sh");

    if !path.exists() {
        println!("No autostart script at {:?}", path);
        return;
    }

    match detached(&format!("exec sh '{}'", path.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => println!("Autostart launched"),
        Err(err) => eprintln!("Error running autostart: {}", err),
    }
}

/// The intermediate shell exits right away, so the command ends up as a child of
/// init and the WM never has to reap it. Wait for it with `status()`
pub fn detached(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "sh -c \"$1\" &", "sh", command]);
    cmd
}

/// Blocks the WM: only for quick commands, and never for ones that leave a
/// background process holding stdout (like xclip)
pub fn command_output(cmd: &str) -> String {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stderr(Stdio::null())
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn volume_status() -> String {
    let volume = command_output("pamixer --get-volume");
    let label = if command_output("pamixer --get-mute") == "true" {
        "MUTED"
    } else {
        "VOL"
    };

    format!("[{}] {}%", label, volume)
}

pub fn mic_status() -> String {
    if command_output("pamixer --default-source --get-mute") == "true" {
        "[MIC] MUTED".to_string()
    } else {
        "[MIC] LIVE".to_string()
    }
}

pub fn battery() -> Option<(&'static str, String)> {
    let read = |file: &str| {
        fs::read_to_string(format!("/sys/class/power_supply/BAT0/{}", file))
            .ok()
            .map(|s| s.trim().to_string())
    };

    let capacity = read("capacity")?;
    let label = match read("status").as_deref() {
        Some("Charging") => "CHR",
        Some("Discharging") => "BAT",
        Some("Full") => "FULL",
        Some("Not charging") => "AC",
        _ => "UNK",
    };

    Some((label, capacity))
}

pub fn battery_status() -> String {
    match battery() {
        Some((label, capacity)) => format!("[{}] {}%", label, capacity),
        None => "[BAT] none".to_string(),
    }
}

pub fn local_time() -> libc::tm {
    // SAFETY: localtime_r only writes to the tm we own
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    }
}
