mod draw;
mod input;
mod launcher;
mod output;

use std::cell::Cell;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::time::{Duration, Instant};

use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME};

use crate::config::keybinds::COMMANDS;
use crate::config::{
    BACKGROUND, BORDER_FOCUSED, BORDER_WIDTH, MARGIN, PROMPT_OUTPUT_LINES, PROMPT_WIDTH,
    TITLE_PADDING,
};
use crate::keybindings::Action;
use crate::keyboard::keysym_to_char;
use crate::keysyms::*;
use crate::utils::detached;
use crate::wm::WindowManager;

use self::launcher::{executables, read_history, write_history};
use self::output::Output;

pub const LINE_PADDING: i16 = 3;
const BLINK: Duration = Duration::from_millis(530);

#[derive(Clone, Copy)]
enum Entry {
    Run,
    Focus(Window),
    Call(Action),
}

pub struct Prompt {
    pub window: Window,
    width: u16,
    label: String,
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
    blink: Instant,
    cursor_shown: Cell<bool>,
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

        let label = shell_label(if capture { "out" } else { "run" });
        self.open_prompt(label, entries, true, capture)
    }

    pub fn prompt_clients(&mut self) -> Result<()> {
        let several_monitors = self.monitors.count() > 1;
        let mut entries = Vec::new();

        for (monitor_id, monitor) in self.monitors.monitors.iter().enumerate() {
            for workspace in &monitor.workspaces.workspaces {
                for client in &workspace.clients {
                    let label = if several_monitors {
                        format!("[{}:{}] {}", monitor_id, workspace.number(), client.title)
                    } else {
                        format!("[{}] {}", workspace.number(), client.title)
                    };
                    entries.push((label, Entry::Focus(client.window)));
                }
            }
        }

        self.open_prompt(shell_label("win"), entries, false, false)
    }

    pub fn prompt_commands(&mut self) -> Result<()> {
        let entries = COMMANDS
            .iter()
            .map(|&(name, command)| (name.to_string(), Entry::Call(command)))
            .collect();

        self.open_prompt(shell_label("cmd"), entries, false, false)
    }

    fn open_prompt(
        &mut self,
        label: String,
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

        let monitor = self.monitors.current().geometry;
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
            blink: Instant::now(),
            cursor_shown: Cell::new(true),
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

    pub fn poll_prompt(&mut self) -> Result<()> {
        let Some(prompt) = self.prompt.as_mut() else {
            return Ok(());
        };
        let (columns, visible) = (prompt.columns, prompt.output_lines);

        let blinked =
            prompt.output.is_none() && prompt.cursor_visible() != prompt.cursor_shown.get();
        let changed = prompt
            .output
            .as_mut()
            .is_some_and(|output| output.refresh(columns, visible));

        if changed || blinked {
            self.draw_prompt()
        } else {
            Ok(())
        }
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

        prompt.blink = Instant::now();
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
            Some((_, Entry::Call(command))) => command(self)?,
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
            prompt.label = shell_label("out");
            prompt.input = command.chars().collect();
            prompt.cursor = 0;
            prompt.output = Some(output);
        }
        self.draw_prompt()
    }

    pub fn run_command(&mut self, command: &str, remember: bool) {
        println!("Spawning: {}", command);

        if let Err(err) = detached(command).status() {
            eprintln!("Error running {}: {}", command, err);
            return;
        }

        if remember {
            write_history(command);
        }
    }
}

fn shell_label(mode: &str) -> String {
    let user = env::var("USER").unwrap_or_else(|_| "dxwm".to_string());
    let host = fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    format!("{}@{}:{}$ ", user, host.trim(), mode)
}
