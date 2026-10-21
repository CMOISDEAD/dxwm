use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{KeyPressEvent, ModMask};

use super::{COLUMN_WIDTH_STEP, EDITOR_APP, FILEMANAGER_APP, TERMINAL_APP};
use crate::keybindings::Action;
use crate::keybindings::KeyAction::{self, Custom};
use crate::keyboard::{self, normalize_modifiers};
use crate::keysyms::*;
use crate::screenshot::Shot;
use crate::utils::{battery_status, command_output, mic_status, volume_status};
use crate::wm::WindowManager;

impl WindowManager {
    pub fn setup_keybindings(&mut self) -> Result<()> {
        let keymap = &self.keymap;
        const SUPER: ModMask = ModMask::M4;
        let super_shift = ModMask::M4 | ModMask::SHIFT;
        let none = ModMask::default();
        let keybindings = &mut self.keybindings;

        let mut bind = |keysym: u32, modifiers: ModMask, action: KeyAction| {
            if let Some(keycode) = keymap.keycode(keysym) {
                keybindings.bind_normal(keycode, modifiers, action);
            }
        };

        let digits = [XK_1, XK_2, XK_3, XK_4, XK_5, XK_6, XK_7, XK_8, XK_9];
        for (workspace_id, keysym) in digits.into_iter().enumerate() {
            bind(keysym, SUPER, KeyAction::SwitchWorkspace(workspace_id));
            bind(
                keysym,
                super_shift,
                KeyAction::MoveToWorkspace(workspace_id),
            );
        }

        bind(XK_TAB, SUPER, Custom(WindowManager::cycle_last_workspace));

        bind(XK_COMMA, SUPER, Custom(WindowManager::focus_prev_monitor));
        bind(XK_PERIOD, SUPER, Custom(WindowManager::focus_next_monitor));
        bind(
            XK_COMMA,
            super_shift,
            Custom(WindowManager::move_focused_to_prev_monitor),
        );
        bind(
            XK_PERIOD,
            super_shift,
            Custom(WindowManager::move_focused_to_next_monitor),
        );
        // `xrandr --setmonitor` sends no RandR events
        bind(XK_R, super_shift, Custom(monitors));

        bind(XK_J, SUPER, Custom(WindowManager::focus_next));
        bind(XK_K, SUPER, Custom(WindowManager::focus_prev));
        bind(XK_J, super_shift, Custom(WindowManager::swap_next));
        bind(XK_K, super_shift, Custom(WindowManager::swap_prev));
        bind(XK_F, SUPER, Custom(fullscreen));
        bind(XK_SPACE, super_shift, Custom(float));
        bind(
            XK_C,
            super_shift,
            Custom(WindowManager::close_focused_client),
        );

        bind(
            XK_H,
            SUPER,
            Custom(|wm| wm.resize_column(-COLUMN_WIDTH_STEP)),
        );
        bind(
            XK_L,
            SUPER,
            Custom(|wm| wm.resize_column(COLUMN_WIDTH_STEP)),
        );
        bind(XK_R, SUPER, Custom(WindowManager::cycle_column_width));
        bind(XK_M, SUPER, Custom(WindowManager::toggle_maximize_column));
        bind(XK_C, SUPER, Custom(WindowManager::center_column));

        bind(XK_P, SUPER, Custom(|wm| wm.prompt_run(false)));
        bind(XK_P, super_shift, Custom(|wm| wm.prompt_run(true)));
        bind(XK_W, SUPER, Custom(WindowManager::prompt_clients));
        bind(XK_SEMICOLON, SUPER, Custom(WindowManager::prompt_commands));

        bind(XK_RETURN, SUPER, Custom(terminal));
        bind(XK_GRAVE, SUPER, Custom(WindowManager::toggle_scratchpad));
        bind(XK_A, SUPER, KeyAction::EnterMode("apps"));
        bind(XK_S, SUPER, KeyAction::EnterMode("alerts"));
        bind(XK_B, SUPER, Custom(WindowManager::banish_pointer));
        bind(XK_G, SUPER, Custom(WindowManager::clear_alerts));
        bind(XK_ESCAPE, super_shift, Custom(WindowManager::quit));
        bind(
            XK_R,
            SUPER | ModMask::CONTROL,
            Custom(WindowManager::restart),
        );

        bind(
            XK_AUDIO_RAISE_VOL,
            none,
            Custom(|wm| {
                command_output("pamixer -i 10");
                wm.draw_alert(volume_status())
            }),
        );
        bind(
            XK_AUDIO_LOWER_VOL,
            none,
            Custom(|wm| {
                command_output("pamixer -d 10");
                wm.draw_alert(volume_status())
            }),
        );
        bind(
            XK_AUDIO_MUTE,
            none,
            Custom(|wm| {
                command_output("pamixer -t");
                wm.draw_alert(volume_status())
            }),
        );
        bind(
            XK_AUDIO_MIC_MUTE,
            none,
            Custom(|wm| {
                command_output("pamixer --default-source -t");
                wm.draw_alert(mic_status())
            }),
        );

        bind(XK_PRINT, none, Custom(screenshot_monitor));
        bind(XK_PRINT, ModMask::SHIFT, Custom(screenshot_area));
        bind(XK_PRINT, SUPER, Custom(screenshot_client));

        let submaps: [(&str, Vec<(u32, KeyAction)>); 2] = [
            (
                "apps",
                vec![
                    (XK_T, Custom(terminal)),
                    (XK_E, Custom(editor)),
                    (XK_F, Custom(files)),
                ],
            ),
            (
                "alerts",
                vec![
                    (XK_B, Custom(battery)),
                    (XK_V, Custom(volume)),
                    (XK_D, Custom(date)),
                ],
            ),
        ];

        for (mode, bindings) in submaps {
            self.keybindings.add_submap(mode, true);

            for (keysym, action) in bindings
                .into_iter()
                .chain([(XK_ESCAPE, KeyAction::ExitMode)])
            {
                if let Some(keycode) = keymap.keycode(keysym) {
                    self.keybindings.bind_in_mode(mode, keycode, action);
                }
            }
        }

        self.grab_buttons()?;
        self.update_grabs()
    }

    fn update_grabs(&self) -> Result<()> {
        keyboard::ungrab_all_keys(&self.conn, self.root)?;

        if self.keybindings.is_in_submap() {
            keyboard::grab_keyboard(&self.conn, self.root)?;
        } else {
            for binding in self.keybindings.active_bindings() {
                keyboard::grab_key(&self.conn, self.root, binding.keycode, binding.modifiers)?;
            }
        }

        self.conn.flush()?;
        Ok(())
    }

    fn execute_action(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::EnterMode(mode) => {
                self.draw_alert(format!("[MODE] {}", mode.to_ascii_uppercase()))?;
                self.keybindings.enter_mode(mode);
                self.update_grabs()?;
            }
            KeyAction::ExitMode => {
                self.draw_alert("[MODE] NORMAL".to_string())?;
                self.keybindings.exit_mode();
                self.update_grabs()?;
            }
            Custom(func) => func(self)?,
            KeyAction::SwitchWorkspace(index) => {
                if self.switch_to_workspace(index)? {
                    self.draw_alert(format!("[WS] {}", index + 1))?;
                }
            }
            KeyAction::MoveToWorkspace(index) => {
                if self.move_focused_to_workspace(index)? {
                    self.draw_alert(format!("[MVWS] {}", index + 1))?;
                }
            }
        }
        Ok(())
    }

    pub fn handle_key_press(&mut self, event: &KeyPressEvent) -> Result<()> {
        let modifiers = normalize_modifiers(ModMask::from(u16::from(event.state)));

        match self.keybindings.find_action(event.detail, modifiers) {
            Some(action) => {
                let should_exit =
                    self.keybindings.should_auto_exit() && !matches!(action, KeyAction::ExitMode);

                let result = self.execute_action(action);

                if should_exit {
                    self.keybindings.exit_mode();
                    self.update_grabs()?;
                }
                result?;
            }
            None if self.keybindings.is_in_submap() => {
                self.keybindings.exit_mode();
                self.update_grabs()?;
            }
            None => {}
        }

        Ok(())
    }
}

pub const COMMANDS: &[(&str, Action)] = &[
    ("terminal", terminal),
    ("editor", editor),
    ("files", files),
    ("scratchpad", WindowManager::toggle_scratchpad),
    ("fullscreen", fullscreen),
    ("float", float),
    ("maximize", WindowManager::toggle_maximize_column),
    ("center", WindowManager::center_column),
    ("close", WindowManager::close_focused_client),
    ("screenshot", screenshot_monitor),
    ("screenshot area", screenshot_area),
    ("screenshot client", screenshot_client),
    ("battery", battery),
    ("volume", volume),
    ("date", date),
    ("monitors", monitors),
    ("restart", WindowManager::restart),
    ("quit", WindowManager::quit),
];

fn terminal(wm: &mut WindowManager) -> Result<()> {
    wm.run_command(TERMINAL_APP, false);
    Ok(())
}

fn editor(wm: &mut WindowManager) -> Result<()> {
    wm.run_command(EDITOR_APP, false);
    Ok(())
}

fn files(wm: &mut WindowManager) -> Result<()> {
    wm.run_command(FILEMANAGER_APP, false);
    Ok(())
}

fn fullscreen(wm: &mut WindowManager) -> Result<()> {
    match wm.focused_client() {
        Some(window) => wm.toggle_fullscreen(window),
        None => Ok(()),
    }
}

fn float(wm: &mut WindowManager) -> Result<()> {
    match wm.focused_client() {
        Some(window) => wm.toggle_floating(window),
        None => Ok(()),
    }
}

fn screenshot_monitor(wm: &mut WindowManager) -> Result<()> {
    wm.screenshot(Shot::Monitor)
}

fn screenshot_area(wm: &mut WindowManager) -> Result<()> {
    wm.screenshot(Shot::Area)
}

fn screenshot_client(wm: &mut WindowManager) -> Result<()> {
    wm.screenshot(Shot::Window)
}

fn battery(wm: &mut WindowManager) -> Result<()> {
    wm.draw_alert(battery_status())
}

fn volume(wm: &mut WindowManager) -> Result<()> {
    wm.draw_alert(volume_status())
}

fn date(wm: &mut WindowManager) -> Result<()> {
    let date = command_output("date '+%a %d %b %H:%M'");
    wm.draw_alert(format!("[DATE] {}", date))
}

fn monitors(wm: &mut WindowManager) -> Result<()> {
    wm.refresh_monitors()?;
    wm.draw_alert(format!("[MON] {} detected", wm.monitors.count()))
}
