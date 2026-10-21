use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

use super::LINE_PADDING;
use super::input::{fold, matched_columns};
use crate::config::{
    BACKGROUND, FOREGROUND, PROMPT_LINES, SELECTED_BG, SELECTED_FG, TITLE_BG_FOCUSED,
    TITLE_FG_FOCUSED, TITLE_PADDING,
};
use crate::decorations::encode_text;
use crate::wm::WindowManager;

impl WindowManager {
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
                    None => "[RUNNING]".to_string(),
                    Some(status) if status.success() => String::new(),
                    Some(status) => match status.code() {
                        Some(code) => format!("[EXIT {}]", code),
                        None => "[KILLED]".to_string(),
                    },
                };
                let position = if count > prompt.output_lines {
                    let w = digits(count);
                    format!("[{:0w$}-{:0w$}/{:0w$}]", top + 1, last, count)
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
                let count = prompt.matches.len();
                let current = if count == 0 { 0 } else { prompt.selected + 1 };
                let w = digits(count);
                (
                    lines,
                    Some(prompt.selected - prompt.top),
                    format!("[{:0w$}/{:0w$}]", current, count),
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
            .saturating_sub(prompt.label.chars().count() + status.len() + 1)
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

        let cursor_visible = prompt.output.is_none() && prompt.cursor_visible();
        prompt.cursor_shown.set(cursor_visible);
        if cursor_visible {
            let cursor_column = prompt.label.chars().count() + prompt.cursor - start;
            let under = prompt.input.get(prompt.cursor).copied().unwrap_or(' ');
            decorations.draw_text(
                &self.conn,
                prompt.window,
                (TITLE_PADDING + cursor_column as i16 * char_width, 0),
                line_height,
                (TITLE_BG_FOCUSED, TITLE_FG_FOCUSED),
                &encode_text(&under.to_string(), 1),
            )?;
        }

        let words: Vec<Vec<char>> = prompt
            .text()
            .split_whitespace()
            .map(|word| word.chars().map(fold).collect())
            .collect();

        for (row, text) in lines.iter().enumerate() {
            let y = line_height * (row as i16 + 1);
            let (foreground, background) = if selected == Some(row) {
                (SELECTED_FG, SELECTED_BG)
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

            if prompt.output.is_some() {
                continue;
            }

            let shown = if text.chars().count() > columns {
                columns.saturating_sub(3)
            } else {
                columns
            };
            for column in matched_columns(text, &words) {
                if column >= shown {
                    continue;
                }
                decorations.fill(
                    &self.conn,
                    prompt.window,
                    Rectangle {
                        x: TITLE_PADDING + column as i16 * char_width,
                        y: y + line_height - LINE_PADDING,
                        width: char_width as u16,
                        height: 1,
                    },
                    foreground,
                )?;
            }
        }

        self.conn.flush()?;
        Ok(())
    }
}

fn digits(count: usize) -> usize {
    count.to_string().len().max(3)
}
