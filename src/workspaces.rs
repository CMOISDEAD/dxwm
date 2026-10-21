use anyhow::Result;
use x11rb::protocol::xproto::Window;

use crate::clients::Client;
use crate::wm::WindowManager;

#[derive(Debug, Clone)]
pub struct Workspace {
    /// Index in its monitor, shown to the user as `number()`
    pub id: usize,
    pub clients: Vec<Client>,
    pub focused_client: Option<Window>,
    pub scroll: i32,
}

impl Workspace {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            clients: Vec::new(),
            focused_client: None,
            scroll: 0,
        }
    }

    pub fn number(&self) -> usize {
        self.id + 1
    }

    pub fn position(&self, window: Window) -> Option<usize> {
        self.clients.iter().position(|c| c.window == window)
    }

    /// Positions of the focused client and of the one `step` away from it, clamped
    /// to the ends. None if there is nowhere to go
    pub fn step_from_focused(&self, step: isize) -> Option<(usize, usize)> {
        let current = self.focused_client.and_then(|w| self.position(w))?;
        let target = current
            .saturating_add_signed(step)
            .min(self.clients.len() - 1);
        (target != current).then_some((current, target))
    }

    pub fn get(&self, window: Window) -> Option<&Client> {
        self.clients.iter().find(|c| c.window == window)
    }

    pub fn get_mut(&mut self, window: Window) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.window == window)
    }

    pub fn windows(&self) -> Vec<Window> {
        self.clients.iter().map(|c| c.window).collect()
    }

    /// Positions of the clients that aren't floating, grouped by column. Fullscreen
    /// clients keep their place here but are not laid out
    fn groups(&self) -> Vec<Vec<usize>> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (idx, client) in self.clients.iter().enumerate() {
            if client.floating.is_some() {
                continue;
            }
            match groups.last_mut() {
                Some(group) if client.stacked => group.push(idx),
                _ => groups.push(vec![idx]),
            }
        }
        groups
    }

    pub fn columns(&self) -> Vec<Vec<Window>> {
        self.groups()
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|idx| &self.clients[idx])
                    .filter(|c| !c.fullscreen)
                    .map(|c| c.window)
                    .collect::<Vec<_>>()
            })
            .filter(|column| !column.is_empty())
            .collect()
    }

    pub fn column_head(&self, window: Window) -> Option<Window> {
        self.columns()
            .into_iter()
            .find(|column| column.contains(&window))
            .map(|column| column[0])
    }

    fn group_of(groups: &[Vec<usize>], idx: usize) -> Option<usize> {
        groups.iter().position(|group| group.contains(&idx))
    }

    /// The next client of its column takes its place
    pub fn leave_column(&mut self, window: Window) {
        let Some(idx) = self.position(window) else {
            return;
        };

        if self.clients[idx].floating.is_none() && !self.clients[idx].stacked {
            let next = self.clients[idx + 1..]
                .iter_mut()
                .find(|c| c.floating.is_none());
            if let Some(next) = next {
                next.stacked = false;
            }
        }
        self.clients[idx].stacked = false;
    }

    /// Alone in its column it joins the next column in that direction, otherwise it
    /// leaves its column to a new one on that side
    pub fn consume_or_expel(&mut self, window: Window, right: bool) -> bool {
        let groups = self.groups();
        let Some(idx) = self.position(window) else {
            return false;
        };
        let Some(group) = Self::group_of(&groups, idx) else {
            return false;
        };

        if groups[group].len() > 1 {
            self.leave_column(window);
            let client = self.clients.remove(idx);
            let groups = self.groups();
            let at = if right {
                groups[group].last().map_or(idx, |&last| last + 1)
            } else {
                groups[group][0]
            };
            self.clients.insert(at, client);
            return true;
        }

        let target = if right {
            group + 1
        } else {
            let Some(target) = group.checked_sub(1) else {
                return false;
            };
            target
        };
        if target >= groups.len() {
            return false;
        }

        let mut client = self.clients.remove(idx);
        client.stacked = true;
        let groups = self.groups();
        let target = if right { group } else { target };
        let at = groups[target].last().map_or(idx, |&last| last + 1);
        self.clients.insert(at, client);
        true
    }

    /// Swaps it with its neighbour in the column, or moves its whole column past the
    /// next one. The shape and width of the columns stay where they are
    pub fn move_window(&mut self, window: Window, forward: bool) -> bool {
        let groups = self.groups();
        let Some(idx) = self.position(window) else {
            return false;
        };

        let Some(group) = Self::group_of(&groups, idx) else {
            let other = if forward {
                idx + 1
            } else {
                let Some(other) = idx.checked_sub(1) else {
                    return false;
                };
                other
            };
            if other >= self.clients.len() {
                return false;
            }
            self.swap_keeping_columns(idx, other);
            return true;
        };

        let members = &groups[group];
        let row = members.iter().position(|&i| i == idx).unwrap_or(0);
        let neighbour = if forward {
            members.get(row + 1)
        } else {
            row.checked_sub(1).and_then(|r| members.get(r))
        };
        if let Some(&other) = neighbour {
            self.swap_keeping_columns(idx, other);
            return true;
        }

        let target = if forward {
            group + 1
        } else {
            let Some(target) = group.checked_sub(1) else {
                return false;
            };
            target
        };
        if target >= groups.len() {
            return false;
        }

        let block: Vec<Client> = members
            .iter()
            .rev()
            .map(|&i| self.clients.remove(i))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let groups = self.groups();
        let at = if forward {
            groups[group]
                .last()
                .map_or(self.clients.len(), |&last| last + 1)
        } else {
            groups[target][0]
        };
        self.clients.splice(at..at, block);
        true
    }

    fn swap_keeping_columns(&mut self, a: usize, b: usize) {
        self.clients.swap(a, b);
        let (first, second) = (&self.clients[a], &self.clients[b]);
        let swapped = (
            (second.stacked, second.column_width),
            (first.stacked, first.column_width),
        );
        (self.clients[a].stacked, self.clients[a].column_width) = swapped.0;
        (self.clients[b].stacked, self.clients[b].column_width) = swapped.1;
    }

    pub fn add_client(&mut self, mut client: Client) {
        client.stacked = false;
        let groups = self.groups();
        let idx = match self.focused_client.and_then(|w| self.position(w)) {
            Some(focused) if client.floating.is_none() => {
                Self::group_of(&groups, focused)
                    .and_then(|group| groups[group].last())
                    .map_or(focused, |&last| last)
                    + 1
            }
            Some(focused) => focused + 1,
            None => self.clients.len(),
        };

        if self.focused_client.is_none() {
            self.focused_client = Some(client.window);
        }
        self.clients.insert(idx, client);
    }

    pub fn remove_client(&mut self, window: Window) -> Option<Client> {
        self.leave_column(window);
        let idx = self.position(window)?;
        let client = self.clients.remove(idx);

        if self.focused_client == Some(window) {
            let next = match client.floating {
                Some(_) => idx.saturating_sub(1),
                None => idx,
            };

            self.focused_client = self
                .clients
                .get(next)
                .or_else(|| self.clients.last())
                .map(|c| c.window);
        }

        Some(client)
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub current_workspace: usize,
    pub last_workspace: usize,
}

impl WorkspaceManager {
    pub fn new(num_workspaces: usize) -> Self {
        Self {
            workspaces: (0..num_workspaces).map(Workspace::new).collect(),
            current_workspace: 0,
            last_workspace: 1,
        }
    }

    pub fn current(&self) -> &Workspace {
        &self.workspaces[self.current_workspace]
    }

    pub fn current_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.current_workspace]
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Workspace> {
        self.workspaces.get_mut(id)
    }

    pub fn contains(&self, id: usize) -> bool {
        id < self.workspaces.len()
    }
}

impl WindowManager {
    pub fn switch_to_workspace(&mut self, workspace_id: usize) -> Result<bool> {
        let workspaces = &self.monitors.current().workspaces;
        if !workspaces.contains(workspace_id) || workspaces.current_workspace == workspace_id {
            return Ok(false);
        }

        println!("Switching to workspace {}", workspace_id + 1);

        for window in self.workspace().windows() {
            self.hide_client(window)?;
        }

        let workspaces = &mut self.monitors.current_mut().workspaces;
        workspaces.last_workspace = workspaces.current_workspace;
        workspaces.current_workspace = workspace_id;

        self.layout()?;

        for window in self.workspace().windows() {
            self.show_client(window)?;
        }

        self.focus_current()?;
        Ok(true)
    }

    pub fn move_focused_to_workspace(&mut self, workspace_id: usize) -> Result<bool> {
        let workspaces = &self.monitors.current().workspaces;
        if !workspaces.contains(workspace_id) || workspaces.current_workspace == workspace_id {
            return Ok(false);
        }

        let Some(window) = self.focused_client() else {
            return Ok(false);
        };

        println!("Moving client {} to workspace {}", window, workspace_id + 1);

        self.hide_client(window)?;

        let workspaces = &mut self.monitors.current_mut().workspaces;
        if let Some(client) = workspaces.current_mut().remove_client(window)
            && let Some(target) = workspaces.get_mut(workspace_id)
        {
            target.add_client(client);
        }

        self.layout()?;
        self.focus_current()?;
        Ok(true)
    }

    pub fn cycle_last_workspace(&mut self) -> Result<()> {
        let last = self.monitors.current().workspaces.last_workspace;
        self.switch_to_workspace(last)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::Rect;

    fn workspace(count: usize) -> Workspace {
        let mut ws = Workspace::new(0);
        for window in 1..=count as Window {
            ws.add_client(Client::new(window, window + 100, String::new(), None));
            ws.focused_client = Some(window);
        }
        ws
    }

    #[test]
    fn consume_joins_the_neighbour_column_and_expel_leaves_it() {
        let mut ws = workspace(3);

        assert!(ws.consume_or_expel(2, false));
        assert_eq!(ws.columns(), vec![vec![1, 2], vec![3]]);

        assert!(ws.consume_or_expel(3, false));
        assert_eq!(ws.columns(), vec![vec![1, 2, 3]]);

        assert!(ws.consume_or_expel(1, true));
        assert_eq!(ws.columns(), vec![vec![2, 3], vec![1]]);

        assert!(ws.consume_or_expel(1, false));
        assert_eq!(ws.columns(), vec![vec![2, 3, 1]]);

        assert!(ws.consume_or_expel(3, false));
        assert_eq!(ws.columns(), vec![vec![3], vec![2, 1]]);
    }

    #[test]
    fn moving_a_window_swaps_inside_its_column_keeping_the_width() {
        let mut ws = workspace(3);
        ws.consume_or_expel(3, false);
        ws.get_mut(2).unwrap().column_width = 0.7;

        assert!(ws.move_window(3, false));
        assert_eq!(ws.columns(), vec![vec![1], vec![3, 2]]);
        assert_eq!(ws.get(3).unwrap().column_width, 0.7);
    }

    #[test]
    fn moving_past_the_column_end_moves_the_whole_column() {
        let mut ws = workspace(3);
        ws.consume_or_expel(3, false);

        assert!(ws.move_window(2, false));
        assert_eq!(ws.columns(), vec![vec![2, 3], vec![1]]);
        assert!(ws.move_window(3, true));
        assert_eq!(ws.columns(), vec![vec![1], vec![2, 3]]);
        assert!(!ws.move_window(3, true));
    }

    #[test]
    fn consume_into_the_right_column() {
        let mut ws = workspace(3);
        assert!(ws.consume_or_expel(1, true));
        assert_eq!(ws.columns(), vec![vec![2, 1], vec![3]]);
        assert!(!workspace(1).consume_or_expel(1, true));
    }

    #[test]
    fn the_next_window_takes_the_place_of_a_removed_head() {
        let mut ws = workspace(3);
        ws.consume_or_expel(2, false);
        ws.consume_or_expel(3, false);

        ws.remove_client(1);
        assert_eq!(ws.columns(), vec![vec![2, 3]]);
    }

    #[test]
    fn new_windows_open_after_the_focused_column() {
        let mut ws = workspace(3);
        ws.consume_or_expel(2, false);
        ws.focused_client = Some(1);

        ws.add_client(Client::new(4, 104, String::new(), None));
        assert_eq!(ws.columns(), vec![vec![1, 2], vec![4], vec![3]]);
    }

    #[test]
    fn a_fullscreen_head_keeps_its_column_together() {
        let mut ws = workspace(3);
        ws.consume_or_expel(3, false);
        ws.get_mut(2).unwrap().fullscreen = true;

        assert_eq!(ws.columns(), vec![vec![1], vec![3]]);
        ws.get_mut(2).unwrap().fullscreen = false;
        assert_eq!(ws.columns(), vec![vec![1], vec![2, 3]]);
    }

    #[test]
    fn floating_clients_are_not_part_of_columns() {
        let mut ws = workspace(2);
        ws.consume_or_expel(2, false);
        ws.focused_client = Some(1);
        let dialog = Client::new(3, 103, String::new(), Some(Rect::default()));
        ws.add_client(dialog);

        assert_eq!(ws.position(3), Some(1));
        assert_eq!(ws.columns(), vec![vec![1, 2]]);
    }
}
