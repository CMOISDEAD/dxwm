use crate::wm::WindowManager;
use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::CURRENT_TIME;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutType {
    MasterStack,
    Monocle,
    Floating,
    Grid,
}

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub current: LayoutType,
    pub master_ratio: f32,
    pub nmaster: usize,
    pub gap_size: i16,
    pub screen_padding: i16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            current: LayoutType::MasterStack,
            master_ratio: 0.5,
            nmaster: 1,
            gap_size: 10,
            screen_padding: 10,
        }
    }
}

impl WindowManager {
    pub fn layout(&mut self) -> Result<()> {
        if self.clients().is_empty() {
            return Ok(());
        }

        let workspace_layout = self.workspaces.current().layout_config.clone();

        match workspace_layout.current {
            LayoutType::MasterStack => self.apply_master_stack_layout()?,
            LayoutType::Monocle => self.apply_monocle_layout()?,
            LayoutType::Floating => {}
            LayoutType::Grid => self.apply_grid_layout()?,
        }

        self.restack_alerts()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn apply_master_stack_layout(&mut self) -> Result<()> {
        let screen = self.conn.setup().roots.get(0).unwrap();

        let workspace = self.workspaces.current();

        let screen_x = workspace.layout_config.screen_padding;
        let screen_y = workspace.layout_config.screen_padding;
        let screen_width =
            screen.width_in_pixels as i16 - (workspace.layout_config.screen_padding * 2);
        let screen_height =
            screen.height_in_pixels as i16 - (workspace.layout_config.screen_padding * 2);

        let gap = workspace.layout_config.gap_size;
        let nmaster = workspace.layout_config.nmaster;

        let mut windows: Vec<Window> = self.clients().keys().copied().collect();
        windows.sort();

        let n_windows = windows.len();

        if n_windows == 0 {
            return Ok(());
        }

        if n_windows == 1 {
            let window = windows[0];
            self.configure_client(window, screen_x, screen_y, screen_width, screen_height)?;
            return Ok(());
        }

        let n_master = nmaster.min(n_windows);
        let n_stack = n_windows - n_master;

        let master_width = if n_stack > 0 {
            ((screen_width as f32 * workspace.layout_config.master_ratio) as i16) - gap
        } else {
            screen_width
        };

        let stack_width = if n_stack > 0 {
            screen_width - master_width - gap
        } else {
            0
        };

        // Master
        if n_master == 1 {
            let window = windows[0];
            self.configure_client(window, screen_x, screen_y, master_width, screen_height)?;
        } else {
            let master_height = (screen_height - (gap * (n_master as i16 - 1))) / n_master as i16;

            for (i, &window) in windows.iter().take(n_master).enumerate() {
                let y = screen_y + (i as i16 * (master_height + gap));
                let h = if i == n_master - 1 {
                    screen_height - (i as i16 * (master_height + gap))
                } else {
                    master_height
                };

                self.configure_client(window, screen_x, y, master_width, h)?;
            }
        }

        // Stack
        if n_stack > 0 {
            let stack_x = screen_x + master_width + gap;
            let stack_height = (screen_height - (gap * (n_stack as i16 - 1))) / n_stack as i16;

            for (i, &window) in windows.iter().skip(n_master).enumerate() {
                let y = screen_y + (i as i16 * (stack_height + gap));
                let h = if i == n_stack - 1 {
                    screen_height - (i as i16 * (stack_height + gap))
                } else {
                    stack_height
                };

                self.configure_client(window, stack_x, y, stack_width, h)?;
            }
        }

        Ok(())
    }

    pub fn apply_monocle_layout(&mut self) -> Result<()> {
        let screen = self.conn.setup().roots.get(0).unwrap();

        let workspace = self.workspaces.current();
        let x = workspace.layout_config.screen_padding;
        let y = workspace.layout_config.screen_padding;
        let width = screen.width_in_pixels as i16 - (workspace.layout_config.screen_padding * 2);
        let height = screen.height_in_pixels as i16 - (workspace.layout_config.screen_padding * 2);

        let windows: Vec<Window> = self.clients().keys().copied().collect();

        for window in windows {
            self.configure_client(window, x, y, width, height)?;
        }

        if let Some(focused) = self.focused_client() {
            self.conn.configure_window(
                focused,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        }

        Ok(())
    }

    pub fn apply_grid_layout(&mut self) -> Result<()> {
        let screen = self.conn.setup().roots.get(0).unwrap();

        let workspace = self.workspaces.current();

        let screen_x = workspace.layout_config.screen_padding;
        let screen_y = workspace.layout_config.screen_padding;
        let screen_width =
            screen.width_in_pixels as i16 - (workspace.layout_config.screen_padding * 2);
        let screen_height =
            screen.height_in_pixels as i16 - (workspace.layout_config.screen_padding * 2);

        let n_windows = self.clients().len();
        if n_windows == 0 {
            return Ok(());
        }

        let cols = (n_windows as f32).sqrt().ceil() as usize;
        let rows = (n_windows as f32 / cols as f32).ceil() as usize;

        let gap = workspace.layout_config.gap_size;
        let cell_width = (screen_width - (gap * (cols as i16 - 1))) / cols as i16;
        let cell_height = (screen_height - (gap * (rows as i16 - 1))) / rows as i16;

        let mut windows: Vec<Window> = self.clients().keys().copied().collect();
        windows.sort();

        for (i, &window) in windows.iter().enumerate() {
            let col = i % cols;
            let row = i / cols;

            let x = screen_x + (col as i16 * (cell_width + gap));
            let y = screen_y + (row as i16 * (cell_height + gap));

            self.configure_client(window, x, y, cell_width, cell_height)?;
        }

        Ok(())
    }

    fn configure_client(
        &mut self,
        window: Window,
        x: i16,
        y: i16,
        width: i16,
        height: i16,
    ) -> Result<()> {
        let width = width.max(50);
        let height = height.max(50);

        self.conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(x as i32)
                .y(y as i32)
                .width(width as u32)
                .height(height as u32),
        )?;

        if let Some(state) = self.clients_mut().get_mut(&window) {
            state.x = x;
            state.y = y;
            state.width = width as u16;
            state.height = height as u16;
        }

        Ok(())
    }

    // TODO: refactor this method
    pub fn next_layout(&mut self) -> Result<()> {
        let workspace = self.workspaces.current_mut();

        workspace.layout_config.current = match workspace.layout_config.current {
            LayoutType::MasterStack => LayoutType::Monocle,
            LayoutType::Monocle => LayoutType::Grid,
            LayoutType::Grid => LayoutType::Floating,
            LayoutType::Floating => LayoutType::MasterStack,
        };
        println!(
            "Layout: {:?}",
            self.workspaces.current().layout_config.current
        );
        self.layout()
    }

    pub fn increase_master_ratio(&mut self) -> Result<()> {
        let workspace = self.workspaces.current_mut();

        workspace.layout_config.master_ratio =
            (workspace.layout_config.master_ratio + 0.05).min(0.95);
        println!("Master ratio: {:.2}", workspace.layout_config.master_ratio);
        self.layout()
    }

    pub fn decrease_master_ratio(&mut self) -> Result<()> {
        let workspace = self.workspaces.current_mut();

        workspace.layout_config.master_ratio =
            (workspace.layout_config.master_ratio - 0.05).max(0.05);
        println!("Master ratio: {:.2}", workspace.layout_config.master_ratio);
        self.layout()
    }

    pub fn increase_nmaster(&mut self) -> Result<()> {
        let workspace = self.workspaces.current_mut();

        workspace.layout_config.nmaster += 1;
        println!("Number of masters: {}", workspace.layout_config.nmaster);
        self.layout()
    }

    pub fn decrease_nmaster(&mut self) -> Result<()> {
        let workspace = self.workspaces.current_mut();

        if workspace.layout_config.nmaster > 1 {
            workspace.layout_config.nmaster -= 1;
            println!("Number of masters: {}", workspace.layout_config.nmaster);
            self.layout()?;
        }
        Ok(())
    }

    pub fn increase_gap(&mut self) -> Result<()> {
        let workspace = self.workspaces.current_mut();

        workspace.layout_config.gap_size += 5;
        println!("Gap size: {}", workspace.layout_config.gap_size);
        self.layout()
    }

    pub fn decrease_gap(&mut self) -> Result<()> {
        let workspace = self.workspaces.current_mut();

        let gap_size = workspace.layout_config.gap_size;
        workspace.layout_config.gap_size = (gap_size - 5).max(0);
        println!(
            "Gap size: {}",
            self.workspaces.current().layout_config.gap_size
        );
        self.layout()
    }

    pub fn rotate_windows(&mut self) -> Result<()> {
        if self.clients().len() < 2 {
            return Ok(());
        }

        let mut windows: Vec<Window> = self.clients().keys().copied().collect();
        windows.sort();

        if let Some(first) = windows.first() {
            let first = *first;

            if let Some(state) = self.clients_mut().remove(&first) {
                self.clients_mut().insert(first, state);
            }

            if self.focused_client() == Some(first) && windows.len() > 1 {
                self.set_focused_client(Some(windows[1]));
                self.conn
                    .set_input_focus(InputFocus::PARENT, windows[1], CURRENT_TIME)?;
            }
        }

        self.layout()
    }

    pub fn promote_to_master(&mut self) -> Result<()> {
        if let Some(focused) = self.focused_client() {
            if let Some(state) = self.clients_mut().remove(&focused) {
                let mut new_clients = std::collections::HashMap::new();
                new_clients.insert(focused, state);

                for (win, st) in self.clients_mut().drain() {
                    new_clients.insert(win, st);
                }

                // FIXME: implement thi
                // self.clients = new_clients;
                self.layout()?;
            }
        }
        Ok(())
    }
}
