use std::ops::Range;

use super::{BLINK, Entry, Prompt};
use crate::config::PROMPT_LINES;

impl Prompt {
    pub(super) fn cursor_visible(&self) -> bool {
        (self.blink.elapsed().as_millis() / BLINK.as_millis()).is_multiple_of(2)
    }

    pub(super) fn text(&self) -> String {
        self.input.iter().collect()
    }

    pub(super) fn filter(&mut self) {
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

    pub(super) fn select(&mut self, step: isize) {
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

    pub(super) fn selected_entry(&self) -> Option<&(String, Entry)> {
        self.matches.get(self.selected).map(|&i| &self.entries[i])
    }

    pub(super) fn insert(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
        self.filter();
    }

    pub(super) fn delete(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }

        self.cursor = range.start;
        self.input.drain(range);
        self.filter();
    }

    pub(super) fn word_start(&self) -> usize {
        let before = &self.input[..self.cursor];
        let word_end = before.iter().rposition(|c| *c != ' ').map_or(0, |i| i + 1);

        before[..word_end]
            .iter()
            .rposition(|c| *c == ' ')
            .map_or(0, |i| i + 1)
    }

    pub(super) fn complete(&mut self) {
        if let Some((label, _)) = self.selected_entry() {
            self.input = label.chars().collect();
            self.cursor = self.input.len();
            self.filter();
        }
    }
}

pub(super) fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

pub(super) fn matched_columns(label: &str, words: &[Vec<char>]) -> Vec<usize> {
    let label: Vec<char> = label.chars().map(fold).collect();

    words
        .iter()
        .filter(|word| !word.is_empty() && word.len() <= label.len())
        .filter_map(|word| {
            label
                .windows(word.len())
                .position(|window| window == word.as_slice())
                .map(|start| start..start + word.len())
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<Vec<char>> {
        text.split_whitespace()
            .map(|word| word.chars().map(fold).collect())
            .collect()
    }

    #[test]
    fn matches_every_word_ignoring_case() {
        assert_eq!(
            matched_columns("FireFox", &words("fox fi")),
            vec![4, 5, 6, 0, 1]
        );
        assert!(matched_columns("firefox", &words("chrome")).is_empty());
    }
}
