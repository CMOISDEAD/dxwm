use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{KeyPressEvent, ModMask};

use super::{COLUMN_WIDTH_STEP, EDITOR_APP, FILEMANAGER_APP, TERMINAL_APP};
use crate::keybindings::{Action, KeyAction};
use crate::keyboard::{self, normalize_modifiers};
use crate::keysyms::*;
use crate::screenshot::Shot;
use crate::utils::{battery_status, command_output, detached, mic_status, volume_status};
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
        for (workspace_id, keysym) in (1..).zip(digits) {
            bind(keysym, SUPER, KeyAction::SwitchWorkspace(workspace_id));
            bind(
                keysym,
                super_shift,
                KeyAction::MoveToWorkspace(workspace_id),
            );
        }

        bind(
            XK_TAB,
            SUPER,
            KeyAction::Custom(|wm| wm.cycle_last_workspace()),
        );

        bind(
            XK_COMMA,
            SUPER,
            KeyAction::Custom(|wm| wm.focus_prev_monitor()),
        );
        bind(
            XK_PERIOD,
            SUPER,
            KeyAction::Custom(|wm| wm.focus_next_monitor()),
        );
        bind(
            XK_COMMA,
            super_shift,
            KeyAction::Custom(|wm| wm.move_focused_to_prev_monitor()),
        );
        bind(
            XK_PERIOD,
            super_shift,
            KeyAction::Custom(|wm| wm.move_focused_to_next_monitor()),
        );
        // `xrandr --setmonitor` sends no RandR events
        bind(
            XK_R,
            super_shift,
            KeyAction::Custom(|wm| {
                wm.refresh_monitors()?;
                wm.draw_alert(format!("[MON] {} detected", wm.monitors.count()))
            }),
        );

        bind(XK_J, SUPER, KeyAction::FocusNext);
        bind(XK_K, SUPER, KeyAction::FocusPrev);
        bind(XK_J, super_shift, KeyAction::Custom(|wm| wm.swap_next()));
        bind(XK_K, super_shift, KeyAction::Custom(|wm| wm.swap_prev()));
        bind(
            XK_F,
            SUPER,
            KeyAction::Custom(|wm| match wm.focused_client() {
                Some(window) => wm.toggle_fullscreen(window),
                None => Ok(()),
            }),
        );
        bind(
            XK_SPACE,
            super_shift,
            KeyAction::Custom(|wm| match wm.focused_client() {
                Some(window) => wm.toggle_floating(window),
                None => Ok(()),
            }),
        );
        bind(XK_C, super_shift, KeyAction::CloseWindow);

        bind(
            XK_H,
            SUPER,
            KeyAction::Custom(|wm| wm.resize_column(-COLUMN_WIDTH_STEP)),
        );
        bind(
            XK_L,
            SUPER,
            KeyAction::Custom(|wm| wm.resize_column(COLUMN_WIDTH_STEP)),
        );
        bind(XK_R, SUPER, KeyAction::Custom(|wm| wm.cycle_column_width()));
        bind(
            XK_M,
            SUPER,
            KeyAction::Custom(|wm| wm.toggle_maximize_column()),
        );

        bind(XK_P, SUPER, KeyAction::Custom(|wm| wm.prompt_run(false)));
        bind(
            XK_P,
            super_shift,
            KeyAction::Custom(|wm| wm.prompt_run(true)),
        );
        bind(XK_W, SUPER, KeyAction::Custom(|wm| wm.prompt_clients()));
        bind(
            XK_SEMICOLON,
            SUPER,
            KeyAction::Custom(|wm| wm.prompt_commands()),
        );

        bind(XK_RETURN, SUPER, KeyAction::Spawn(TERMINAL_APP.to_string()));
        bind(XK_A, SUPER, KeyAction::EnterMode("apps".to_string()));
        bind(XK_S, SUPER, KeyAction::EnterMode("alerts".to_string()));
        bind(XK_B, SUPER, KeyAction::Custom(|wm| wm.banish_pointer()));
        bind(XK_G, SUPER, KeyAction::Custom(|wm| wm.clear_alerts()));
        bind(XK_ESCAPE, super_shift, KeyAction::Quit);
        bind(
            XK_R,
            SUPER | ModMask::CONTROL,
            KeyAction::Custom(|wm| wm.restart()),
        );

        bind(
            XK_AUDIO_RAISE_VOL,
            none,
            KeyAction::Custom(|wm| {
                command_output("pamixer -i 10");
                wm.draw_alert(volume_status())
            }),
        );
        bind(
            XK_AUDIO_LOWER_VOL,
            none,
            KeyAction::Custom(|wm| {
                command_output("pamixer -d 10");
                wm.draw_alert(volume_status())
            }),
        );
        bind(
            XK_AUDIO_MUTE,
            none,
            KeyAction::Custom(|wm| {
                command_output("pamixer -t");
                wm.draw_alert(volume_status())
            }),
        );
        bind(
            XK_AUDIO_MIC_MUTE,
            none,
            KeyAction::Custom(|wm| {
                command_output("pamixer --default-source -t");
                wm.draw_alert(mic_status())
            }),
        );

        bind(
            XK_PRINT,
            none,
            KeyAction::Custom(|wm| wm.screenshot(Shot::Monitor)),
        );
        bind(
            XK_PRINT,
            ModMask::SHIFT,
            KeyAction::Custom(|wm| wm.screenshot(Shot::Area)),
        );
        bind(
            XK_PRINT,
            SUPER,
            KeyAction::Custom(|wm| wm.screenshot(Shot::Window)),
        );

        let submaps: [(&str, Vec<(u32, KeyAction)>); 2] = [
            (
                "apps",
                vec![
                    (XK_T, KeyAction::Spawn(TERMINAL_APP.to_string())),
                    (XK_E, KeyAction::Spawn(EDITOR_APP.to_string())),
                    (XK_F, KeyAction::Spawn(FILEMANAGER_APP.to_string())),
                ],
            ),
            (
                "alerts",
                vec![
                    (
                        XK_B,
                        KeyAction::Custom(|wm| wm.draw_alert(battery_status())),
                    ),
                    (XK_V, KeyAction::Custom(|wm| wm.draw_alert(volume_status()))),
                    (
                        XK_D,
                        KeyAction::Custom(|wm| {
                            let date = command_output("date '+%a %d %b %H:%M'");
                            wm.draw_alert(format!("[DATE] {}", date))
                        }),
                    ),
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
            KeyAction::Spawn(cmd) => {
                println!("Spawning: {}", cmd);
                detached(&cmd).status()?;
            }
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
            KeyAction::CloseWindow => self.close_focused_client()?,
            KeyAction::FocusNext => self.focus_next()?,
            KeyAction::FocusPrev => self.focus_prev()?,
            KeyAction::Custom(func) => func(self)?,
            KeyAction::SwitchWorkspace(index) => {
                if self.switch_to_workspace(index)? {
                    self.draw_alert(format!("[WS] {}", index))?;
                }
            }
            KeyAction::MoveToWorkspace(index) => {
                if self.move_focused_to_workspace(index)? {
                    self.draw_alert(format!("[MVWS] {}", index))?;
                }
            }
            KeyAction::Quit => self.quit()?,
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
    ("terminal", |wm| {
        wm.run_command(TERMINAL_APP, false);
        Ok(())
    }),
    ("editor", |wm| {
        wm.run_command(EDITOR_APP, false);
        Ok(())
    }),
    ("files", |wm| {
        wm.run_command(FILEMANAGER_APP, false);
        Ok(())
    }),
    ("fullscreen", |wm| match wm.focused_client() {
        Some(window) => wm.toggle_fullscreen(window),
        None => Ok(()),
    }),
    ("float", |wm| match wm.focused_client() {
        Some(window) => wm.toggle_floating(window),
        None => Ok(()),
    }),
    ("maximize", |wm| wm.toggle_maximize_column()),
    ("close", |wm| wm.close_focused_client()),
    ("screenshot", |wm| wm.screenshot(Shot::Monitor)),
    ("screenshot area", |wm| wm.screenshot(Shot::Area)),
    ("screenshot client", |wm| wm.screenshot(Shot::Window)),
    ("battery", |wm| wm.draw_alert(battery_status())),
    ("volume", |wm| wm.draw_alert(volume_status())),
    ("date", |wm| {
        let date = command_output("date '+%a %d %b %H:%M'");
        wm.draw_alert(format!("[DATE] {}", date))
    }),
    ("monitors", |wm| {
        wm.refresh_monitors()?;
        wm.draw_alert(format!("[MON] {} detected", wm.monitors.count()))
    }),
    ("restart", |wm| wm.restart()),
    ("quit", |wm| wm.quit()),
];
