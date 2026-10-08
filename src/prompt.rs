use std::collections::HashSet;
use std::env::{self, home_dir};
use std::fs::{self, File, OpenOptions};
use std::ops::Range;
use std::os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{self, Child, Command, ExitStatus, Stdio};

use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME};

use crate::config::keybinds::COMMANDS;
use crate::config::{
    BACKGROUND, BORDER_FOCUSED, BORDER_WIDTH, FOREGROUND, MARGIN, PROMPT_HISTORY, PROMPT_LINES,
    PROMPT_OUTPUT_LINES, PROMPT_WIDTH, SELECTED, TITLE_BG_FOCUSED, TITLE_FG_FOCUSED, TITLE_PADDING,
};
use crate::decorations::encode_text;
use crate::keyboard::keysym_to_char;
use crate::keysyms::*;
use crate::wm::WindowManager;

const LINE_PADDING: i16 = 3;
const OUTPUT_LIMIT: usize = 256 * 1024;

#[derive(Clone, Copy)]
enum Entry {
    Run,
    Focus(Window),
    Call(fn(&mut WindowManager)),
}

pub struct Prompt {
    pub window: Window,
    width: u16,
    label: &'static str,
    entries: Vec<(String, Entry)>,
    matches: Vec<usize>,
    input: Vec<char>,
    cursor: usize,
    selected: usize,
    top: usize,
    shell: bool,
    capture: bool,
    columns: usize,
    output_lines: usize,
    output: Option<Output>,
}

struct Output {
    child: Child,
    file: File,
    size: u64,
    lines: Vec<String>,
    top: usize,
    status: Option<ExitStatus>,
}

impl Output {
    fn run(command: &str) -> Result<Self> {
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

    fn refresh(&mut self, columns: usize, visible: usize) -> bool {
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

    fn scroll(&mut self, step: isize, visible: usize) {
        let last = self.lines.len().saturating_sub(visible);
        self.top = self.top.saturating_add_signed(step).min(last);
    }

    fn stop(&mut self) {
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

impl Prompt {
    fn text(&self) -> String {
        self.input.iter().collect()
    }

    fn filter(&mut self) {
        let input = self.text().to_lowercase();
        let needle = input.trim();
        let words: Vec<&str> = needle.split_whitespace().collect();

        let mut ranked: Vec<(u8, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, (label, _))| {
                let label = label.to_lowercase();
                if !words.iter().all(|word| label.contains(word)) {
                    return None;
                }

                let rank = if label == needle {
                    0
                } else if label.starts_with(needle) {
                    1
                } else {
                    2
                };
                Some((rank, index))
            })
            .collect();
        ranked.sort_unstable();

        self.matches = ranked.into_iter().map(|(_, index)| index).collect();
        self.selected = 0;
        self.top = 0;
    }

    fn select(&mut self, step: isize) {
        if self.matches.is_empty() {
            return;
        }

        let count = self.matches.len() as isize;
        self.selected = (self.selected as isize + step).rem_euclid(count) as usize;

        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + PROMPT_LINES {
            self.top = self.selected + 1 - PROMPT_LINES;
        }
    }

    fn selected_entry(&self) -> Option<&(String, Entry)> {
        self.matches.get(self.selected).map(|&i| &self.entries[i])
    }

    fn insert(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
        self.filter();
    }

    fn delete(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }

        self.cursor = range.start;
        self.input.drain(range);
        self.filter();
    }

    fn word_start(&self) -> usize {
        let before = &self.input[..self.cursor];
        let word_end = before.iter().rposition(|c| *c != ' ').map_or(0, |i| i + 1);

        before[..word_end]
            .iter()
            .rposition(|c| *c == ' ')
            .map_or(0, |i| i + 1)
    }

    fn complete(&mut self) {
        if let Some((label, _)) = self.selected_entry() {
            self.input = label.chars().collect();
            self.cursor = self.input.len();
            self.filter();
        }
    }
}

impl WindowManager {
    pub fn prompt_run(&mut self, capture: bool) -> Result<()> {
        let history = read_history();
        let seen: HashSet<&String> = history.iter().collect();
        let programs: Vec<String> = executables()
            .into_iter()
            .filter(|name| !seen.contains(name))
            .collect();

        let entries = history
            .iter()
            .cloned()
            .chain(programs)
            .map(|command| (command, Entry::Run))
            .collect();

        let label = if capture { "output: " } else { "run: " };
        self.open_prompt(label, entries, true, capture)
    }

    pub fn prompt_clients(&mut self) -> Result<()> {
        let several_monitors = self.monitors.count() > 1;
        let mut entries = Vec::new();

        for (monitor_id, monitor) in self.monitors.monitors.iter().enumerate() {
            for workspace in &monitor.workspaces.workspaces {
                for client in &workspace.clients {
                    let label = if several_monitors {
                        format!("[{}:{}] {}", monitor_id, workspace.id, client.title)
                    } else {
                        format!("[{}] {}", workspace.id, client.title)
                    };
                    entries.push((label, Entry::Focus(client.window)));
                }
            }
        }

        self.open_prompt("client: ", entries, false, false)
    }

    pub fn prompt_commands(&mut self) -> Result<()> {
        let entries = COMMANDS
            .iter()
            .map(|&(name, command)| (name.to_string(), Entry::Call(command)))
            .collect();

        self.open_prompt(": ", entries, false, false)
    }

    fn open_prompt(
        &mut self,
        label: &'static str,
        entries: Vec<(String, Entry)>,
        shell: bool,
        capture: bool,
    ) -> Result<()> {
        self.close_prompt()?;

        let grab = self
            .conn
            .grab_keyboard(
                false,
                self.root,
                CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .reply()?;
        if grab.status != GrabStatus::SUCCESS {
            return self.draw_alert("[PROMPT] keyboard in use".to_string());
        }

        let monitor = self.monitors.current();
        let space = monitor
            .width
            .saturating_sub(2 * (MARGIN + BORDER_WIDTH) as u16);
        let width = PROMPT_WIDTH.min(space).max(1);
        let x = monitor.x + (monitor.width as i16 - width as i16) / 2 - BORDER_WIDTH as i16;
        let offset = monitor.height as i16 / 5;
        let y = monitor.y + offset;

        let line_height = self.decorations.font_height() + 2 * LINE_PADDING;
        let columns = (width as i16 - 2 * TITLE_PADDING) / self.decorations.char_width();

        let below = monitor.height as i16 - offset - (MARGIN + 2 * BORDER_WIDTH) as i16;
        let output_lines = (below / line_height - 1).clamp(1, PROMPT_OUTPUT_LINES as i16);

        let window = self.conn.generate_id()?;
        self.conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            self.root,
            x,
            y,
            width,
            1,
            BORDER_WIDTH as u16,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(BACKGROUND)
                .border_pixel(BORDER_FOCUSED)
                .override_redirect(1)
                .event_mask(EventMask::EXPOSURE),
        )?;
        self.conn.map_window(window)?;

        let mut prompt = Prompt {
            window,
            width,
            label,
            entries,
            matches: Vec::new(),
            input: Vec::new(),
            cursor: 0,
            selected: 0,
            top: 0,
            shell,
            capture,
            columns: columns.max(1) as usize,
            output_lines: output_lines as usize,
            output: None,
        };
        prompt.filter();

        self.prompt = Some(prompt);
        self.draw_prompt()
    }

    pub fn close_prompt(&mut self) -> Result<()> {
        if let Some(mut prompt) = self.prompt.take() {
            if let Some(output) = &mut prompt.output {
                output.stop();
            }
            self.conn.destroy_window(prompt.window)?;
            self.conn.ungrab_keyboard(CURRENT_TIME)?;
            self.conn.flush()?;
        }
        Ok(())
    }

    pub fn draw_prompt(&self) -> Result<()> {
        let Some(prompt) = &self.prompt else {
            return Ok(());
        };

        let decorations = &self.decorations;
        let char_width = decorations.char_width();
        let line_height = decorations.font_height() + 2 * LINE_PADDING;
        let columns = prompt.columns;

        let (lines, selected, status): (Vec<&str>, Option<usize>, String) = match &prompt.output {
            Some(output) => {
                let count = output.lines.len();
                let top = output.top.min(count);
                let last = (top + prompt.output_lines).min(count);

                let state = match output.status {
                    None => "running".to_string(),
                    Some(status) if status.success() => String::new(),
                    Some(status) => match status.code() {
                        Some(code) => format!("exit {}", code),
                        None => "killed".to_string(),
                    },
                };
                let position = if count > prompt.output_lines {
                    format!("{}-{}/{}", top + 1, last, count)
                } else {
                    String::new()
                };

                let mut lines: Vec<&str> =
                    output.lines[top..last].iter().map(String::as_str).collect();
                if lines.is_empty() && output.status.is_some() {
                    lines.push("(no output)");
                }
                (lines, None, [state, position].join(" ").trim().to_string())
            }
            None => {
                let last = (prompt.top + PROMPT_LINES).min(prompt.matches.len());
                let lines = prompt.matches[prompt.top..last]
                    .iter()
                    .map(|&index| prompt.entries[index].0.as_str())
                    .collect();
                (
                    lines,
                    Some(prompt.selected - prompt.top),
                    prompt.matches.len().to_string(),
                )
            }
        };

        self.conn.configure_window(
            prompt.window,
            &ConfigureWindowAux::new().height(line_height as u32 * (1 + lines.len() as u32)),
        )?;

        let line = |y: i16| Rectangle {
            x: 0,
            y,
            width: prompt.width,
            height: line_height as u16,
        };

        let room = columns
            .saturating_sub(prompt.label.len() + status.len() + 1)
            .max(1);
        let start = (prompt.cursor + 1).saturating_sub(room);
        let end = (start + room).min(prompt.input.len());
        let visible: String = prompt.input[start..end].iter().collect();
        let colors = (TITLE_FG_FOCUSED, TITLE_BG_FOCUSED);

        decorations.fill(&self.conn, prompt.window, line(0), TITLE_BG_FOCUSED)?;
        decorations.draw_text(
            &self.conn,
            prompt.window,
            (TITLE_PADDING, 0),
            line_height,
            colors,
            &encode_text(&format!("{}{}", prompt.label, visible), columns),
        )?;
        decorations.draw_text(
            &self.conn,
            prompt.window,
            (
                prompt.width as i16 - TITLE_PADDING - status.len() as i16 * char_width,
                0,
            ),
            line_height,
            colors,
            &encode_text(&status, columns),
        )?;

        if prompt.output.is_none() {
            let cursor_column = prompt.label.len() + prompt.cursor - start;
            decorations.fill(
                &self.conn,
                prompt.window,
                Rectangle {
                    x: TITLE_PADDING + cursor_column as i16 * char_width - 1,
                    y: LINE_PADDING,
                    width: 2,
                    height: decorations.font_height() as u16,
                },
                TITLE_FG_FOCUSED,
            )?;
        }

        for (row, text) in lines.iter().enumerate() {
            let y = line_height * (row as i16 + 1);
            let (foreground, background) = if selected == Some(row) {
                (BACKGROUND, SELECTED)
            } else {
                (FOREGROUND, BACKGROUND)
            };

            decorations.fill(&self.conn, prompt.window, line(y), background)?;
            decorations.draw_text(
                &self.conn,
                prompt.window,
                (TITLE_PADDING, y),
                line_height,
                (foreground, background),
                &encode_text(text, columns),
            )?;
        }

        self.conn.flush()?;
        Ok(())
    }

    pub fn poll_prompt(&mut self) -> Result<()> {
        let Some(prompt) = self.prompt.as_mut() else {
            return Ok(());
        };
        let (columns, visible) = (prompt.columns, prompt.output_lines);

        let changed = prompt
            .output
            .as_mut()
            .is_some_and(|output| output.refresh(columns, visible));

        if changed { self.draw_prompt() } else { Ok(()) }
    }

    pub fn handle_prompt_key(&mut self, event: &KeyPressEvent) -> Result<()> {
        let state = u16::from(event.state);
        let held = |modifiers: ModMask| state & u16::from(modifiers) != 0;
        let ctrl = held(ModMask::CONTROL);

        let keysym = self
            .keymap
            .keysym(event.detail, if ctrl { 0 } else { state });

        let Some(prompt) = self.prompt.as_mut() else {
            return Ok(());
        };

        if let Some(output) = &mut prompt.output {
            let page = prompt.output_lines;
            let step = match (ctrl, keysym) {
                (_, XK_ESCAPE | XK_RETURN | XK_KP_ENTER | XK_Q) | (true, XK_G | XK_C) => {
                    return self.close_prompt();
                }
                (_, XK_DOWN | XK_J) | (true, XK_N) => 1,
                (_, XK_UP | XK_K) | (true, XK_P) => -1,
                (_, XK_PAGE_DOWN | XK_SPACE) | (true, XK_F | XK_V) => page as isize,
                (_, XK_PAGE_UP) | (true, XK_B) => -(page as isize),
                (_, XK_HOME) | (true, XK_A) => isize::MIN,
                (_, XK_END) | (true, XK_E) => isize::MAX,
                _ => return Ok(()),
            };

            output.scroll(step, page);
            return self.draw_prompt();
        }

        let cursor = prompt.cursor;
        let len = prompt.input.len();

        match (ctrl, keysym) {
            (_, XK_ESCAPE) | (true, XK_G | XK_C) => return self.close_prompt(),
            (_, XK_RETURN | XK_KP_ENTER) => return self.accept_prompt(held(ModMask::SHIFT), ctrl),
            (true, XK_J | XK_M) => return self.accept_prompt(false, false),
            (_, XK_TAB) | (true, XK_I) => prompt.complete(),
            (_, XK_DOWN) | (true, XK_N) => prompt.select(1),
            (_, XK_UP) | (true, XK_P) => prompt.select(-1),
            (_, XK_LEFT) | (true, XK_B) => prompt.cursor = cursor.saturating_sub(1),
            (_, XK_RIGHT) | (true, XK_F) => prompt.cursor = (cursor + 1).min(len),
            (_, XK_HOME) | (true, XK_A) => prompt.cursor = 0,
            (_, XK_END) | (true, XK_E) => prompt.cursor = len,
            (_, XK_BACKSPACE) | (true, XK_H) => prompt.delete(cursor.saturating_sub(1)..cursor),
            (_, XK_DELETE) | (true, XK_D) => prompt.delete(cursor..(cursor + 1).min(len)),
            (true, XK_W) => prompt.delete(prompt.word_start()..cursor),
            (true, XK_U) => prompt.delete(0..cursor),
            (true, XK_K) => prompt.delete(cursor..len),
            (false, _) if !held(ModMask::M1 | ModMask::M4) => match keysym_to_char(keysym) {
                Some(c) if held(ModMask::LOCK) && c.is_lowercase() => {
                    prompt.insert(c.to_uppercase().next().unwrap_or(c))
                }
                Some(c) => prompt.insert(c),
                None => return Ok(()),
            },
            _ => return Ok(()),
        }

        self.draw_prompt()
    }

    fn accept_prompt(&mut self, verbatim: bool, capture: bool) -> Result<()> {
        let Some(prompt) = &self.prompt else {
            return Ok(());
        };
        let shell = prompt.shell;
        let capture = shell && (capture || prompt.capture);
        let typed = prompt.text().trim().to_string();
        let selected = prompt
            .selected_entry()
            .filter(|_| !(verbatim && shell))
            .cloned();

        let selected = match selected {
            None if shell && !typed.is_empty() => Some((typed, Entry::Run)),
            selected => selected,
        };

        if capture && let Some((command, Entry::Run)) = &selected {
            return self.show_output(command);
        }

        self.close_prompt()?;

        match selected {
            Some((command, Entry::Run)) => self.run_command(&command, true),
            Some((_, Entry::Focus(window))) => self.jump_to_client(window)?,
            Some((_, Entry::Call(command))) => command(self),
            None => {}
        }
        Ok(())
    }

    fn show_output(&mut self, command: &str) -> Result<()> {
        println!("Spawning: {}", command);

        let output = match Output::run(command) {
            Ok(output) => output,
            Err(err) => {
                eprintln!("Error running {}: {}", command, err);
                self.close_prompt()?;
                return self.draw_alert("[PROMPT] FAILED".to_string());
            }
        };
        write_history(command);

        if let Some(prompt) = self.prompt.as_mut() {
            prompt.label = "$ ";
            prompt.input = command.chars().collect();
            prompt.cursor = 0;
            prompt.output = Some(output);
        }
        self.draw_prompt()
    }

    pub fn run_command(&mut self, command: &str, remember: bool) {
        println!("Spawning: {}", command);

        if let Err(err) = Command::new("sh").arg("-c").arg(command).spawn() {
            eprintln!("Error running {}: {}", command, err);
            return;
        }

        if remember {
            write_history(command);
        }
    }

    fn jump_to_client(&mut self, window: Window) -> Result<()> {
        let Some((monitor_id, workspace_id)) = self.find_client(window) else {
            return Ok(());
        };
        let other_monitor = monitor_id != self.monitors.current_monitor;

        self.focus_monitor(monitor_id, false)?;
        self.switch_to_workspace(workspace_id)?;
        self.set_focused_client(window)?;
        self.layout()?;

        if other_monitor {
            self.warp_pointer_to_focus()?;
        }
        Ok(())
    }
}

fn wrap(text: &str, columns: usize) -> Vec<String> {
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

fn executables() -> Vec<String> {
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

fn read_history() -> Vec<String> {
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

fn write_history(command: &str) {
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
