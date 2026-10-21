use std::env;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::process::{self, Child, Command, ExitStatus, Stdio};

use anyhow::Result;

const OUTPUT_LIMIT: usize = 256 * 1024;

pub(super) struct Output {
    pub(super) child: Child,
    pub(super) file: File,
    pub(super) size: u64,
    pub(super) lines: Vec<String>,
    pub(super) top: usize,
    pub(super) status: Option<ExitStatus>,
}

impl Output {
    pub(super) fn run(command: &str) -> Result<Self> {
        let path = env::temp_dir().join(format!("dxwm-output-{}", process::id()));
        fs::remove_file(&path).ok();
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        fs::remove_file(&path).ok();

        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file.try_clone()?)
            .process_group(0)
            .spawn()?;

        Ok(Self {
            child,
            file,
            size: 0,
            lines: Vec::new(),
            top: 0,
            status: None,
        })
    }

    pub(super) fn refresh(&mut self, columns: usize, visible: usize) -> bool {
        let mut changed = false;

        if self.status.is_none()
            && let Ok(Some(status)) = self.child.try_wait()
        {
            self.status = Some(status);
            changed = true;
        }

        let size = self.file.metadata().map_or(self.size, |meta| meta.len());
        if size != self.size {
            self.size = size;

            let mut bytes = vec![0; (size as usize).min(OUTPUT_LIMIT)];
            let read = self.file.read_at(&mut bytes, 0).unwrap_or(0);
            let following = self.status.is_none() && self.top + visible >= self.lines.len();
            self.lines = wrap(&String::from_utf8_lossy(&bytes[..read]), columns);
            if following {
                self.scroll(isize::MAX, visible);
            }
            changed = true;
        }

        changed
    }

    pub(super) fn scroll(&mut self, step: isize, visible: usize) {
        let last = self.lines.len().saturating_sub(visible);
        self.top = self.top.saturating_add_signed(step).min(last);
    }

    pub(super) fn stop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(None)) {
            return;
        }

        let killed = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.child.id())])
            .status()
            .is_ok_and(|status| status.success());
        if !killed {
            self.child.kill().ok();
        }
        self.child.wait().ok();
    }
}

pub(super) fn wrap(text: &str, columns: usize) -> Vec<String> {
    let columns = columns.clamp(1, 255);
    let mut lines = Vec::new();

    for line in text.lines() {
        let mut chars: Vec<char> = Vec::new();
        for c in line.chars() {
            match c {
                '\t' => chars.extend([' '].repeat(8 - chars.len() % 8)),
                c if c.is_control() => {}
                c => chars.push(c),
            }
        }

        if chars.is_empty() {
            lines.push(String::new());
        }
        lines.extend(
            chars
                .chunks(columns)
                .map(|chunk| chunk.iter().collect::<String>()),
        );
    }

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_expands_tabs_and_splits_long_lines() {
        assert_eq!(wrap("a\tb", 80), vec!["a       b"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_keeps_inner_blank_lines_but_not_trailing_ones() {
        assert_eq!(wrap("a\n\nb\n\n \n", 80), vec!["a", "", "b"]);
    }
}
