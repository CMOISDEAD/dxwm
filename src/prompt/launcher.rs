use std::env::{self, home_dir};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::config::PROMPT_HISTORY;

pub(super) fn executables() -> Vec<String> {
    let path = env::var_os("PATH").unwrap_or_default();

    let mut names: Vec<String> = env::split_paths(&path)
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .filter(|entry| {
            fs::metadata(entry.path())
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();

    names.sort_unstable();
    names.dedup();
    names.sort_by_cached_key(|name| name.to_lowercase());
    names
}

fn history_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".local/share/dxwm/history"))
}

pub(super) fn read_history() -> Vec<String> {
    history_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|history| {
            history
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn write_history(command: &str) {
    let Some(path) = history_path() else {
        return;
    };

    let mut history = read_history();
    history.retain(|other| other != command);
    history.insert(0, command.to_string());
    history.truncate(PROMPT_HISTORY);

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok();
    }
    if let Err(err) = fs::write(&path, history.join("\n") + "\n") {
        eprintln!("Error writing {:?}: {}", path, err);
    }
}
