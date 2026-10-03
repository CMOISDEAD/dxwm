use std::env::home_dir;
use std::process::{Command, Stdio};

/// Run `~/.local/share/dxwm/autostart.sh` in the background, if it exists
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

    match Command::new("sh")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => println!("Autostart launched"),
        Err(err) => eprintln!("Error running autostart: {}", err),
    }
}

/// Run a shell command and wait for its trimmed stdout (empty on failure)
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
