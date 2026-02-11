use std::process::{exit, Command};

use crate::keybindings::KeyAction;
use crate::keyboard::{normalize_modifiers, KeyboardGrabber};
use crate::utils::*;
use crate::wm::WindowManager;
use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ModMask;
use x11rb::protocol::xproto::{ConnectionExt, KeyPressEvent};

impl WindowManager {
    pub fn setup_keybindings(&mut self) -> Result<()> {
        let setup = self.conn.setup();
        let screen = &setup.roots[self.screen_num];
        let grabber = KeyboardGrabber::new(&self.conn, screen, setup)?;

        // === MODO NORMAL ===
        for i in 1..=9 {
            let keysym = match i {
                1 => XK_1,
                2 => XK_2,
                3 => XK_3,
                4 => XK_4,
                5 => XK_5,
                6 => XK_6,
                7 => XK_7,
                8 => XK_8,
                9 => XK_9,
                _ => continue,
            };

            if let Some(key) = grabber.keysym_to_keycode(keysym) {
                let workspace_id = i;
                self.keybindings.bind_normal(
                    key,
                    ModMask::M4,
                    KeyAction::SwitchWorkspace(workspace_id),
                );
            }
        }

        // Super+Shift+1-9: Mover ventana a workspace
        for i in 1..=9 {
            let keysym = match i {
                1 => XK_1,
                2 => XK_2,
                3 => XK_3,
                4 => XK_4,
                5 => XK_5,
                6 => XK_6,
                7 => XK_7,
                8 => XK_8,
                9 => XK_9,
                _ => continue,
            };

            if let Some(key) = grabber.keysym_to_keycode(keysym) {
                let workspace_id = i;
                self.keybindings.bind_normal(
                    key,
                    ModMask::M4 | ModMask::SHIFT,
                    KeyAction::MoveToWorkspace(workspace_id),
                );
            }
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_ESCAPE) {
            self.keybindings
                .bind_normal(key, ModMask::M4 | ModMask::SHIFT, KeyAction::Quit);
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_RETURN) {
            self.keybindings
                .bind_normal(key, ModMask::M4, KeyAction::Spawn("st".to_string()));
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_C) {
            self.keybindings
                .bind_normal(key, ModMask::M4 | ModMask::SHIFT, KeyAction::CloseWindow);
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_N) {
            self.keybindings
                .bind_normal(key, ModMask::M4, KeyAction::EnterMode("nav".to_string()));
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_A) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::EnterMode("apps".to_string()),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_S) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::EnterMode("alerts".to_string()),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_SPACE) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.next_layout().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_J) {
            self.keybindings
                .bind_normal(key, ModMask::M4, KeyAction::FocusNext);
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_K) {
            self.keybindings
                .bind_normal(key, ModMask::M4, KeyAction::FocusPrev);
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_H) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.decrease_master_ratio().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_L) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.increase_master_ratio().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_I) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.increase_nmaster().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_D) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.decrease_nmaster().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_G) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.clear_alerts().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_TAB) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.cycle_last_workspace().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_RETURN) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4 | ModMask::SHIFT,
                KeyAction::Custom(|wm| {
                    wm.promote_to_master().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_EQUAL) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4 | ModMask::SHIFT,
                KeyAction::Custom(|wm| {
                    wm.increase_gap().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_MINUS) {
            self.keybindings.bind_normal(
                key,
                ModMask::M4,
                KeyAction::Custom(|wm| {
                    wm.decrease_gap().ok();
                }),
            );
        }

        // ⭐ EJEMPLO: Binding SIN modificadores
        if let Some(key) = grabber.keysym_to_keycode(XK_AUDIO_RAISE_VOL) {
            self.keybindings.bind_normal(
                key,
                ModMask::default(), // ⭐ SIN MODIFICADORES
                KeyAction::Custom(|wm| {
                    get_command_output("pactl set-sink-volume @DEFAULT_SINK@ +10%");
                    let percentage = get_volume();
                    wm.draw_alert(format!("Volume: {}%", percentage)).ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_AUDIO_LOWER_VOL) {
            self.keybindings.bind_normal(
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    get_command_output("pactl set-sink-volume @DEFAULT_SINK@ -10%");
                    let percentage = get_volume();
                    wm.draw_alert(format!("Volume: {}%", percentage)).ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_AUDIO_MUTE) {
            self.keybindings.bind_normal(
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    let _ = get_command_output("pamixer --toggle-mute");

                    let msg = if is_muted() {
                        "Volume: muted".to_string()
                    } else {
                        format!("Volume: {}%", get_volume())
                    };

                    wm.draw_alert(msg).ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_AUDIO_MIC_MUTE) {
            self.keybindings.bind_normal(
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    let _ = get_command_output("pamixer --default-source --toggle-mute");

                    let muted =
                        get_command_output("pamixer --default-source --get-mute").trim() == "true";

                    let msg = if muted {
                        "Mic: muted".to_string()
                    } else {
                        "Mic: active".to_string()
                    };

                    wm.draw_alert(msg).ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_PRINT) {
            self.keybindings.bind_normal(
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    Command::new("sh")
                        .arg("-c")
                        .arg("maim | xclip -selection clipboard -t image/png -i")
                        .spawn()
                        .unwrap()
                        .wait()
                        .ok();

                    wm.draw_alert("Screenshot copied to cliboard".to_string())
                        .ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_PRINT) {
            self.keybindings.bind_normal(
                key,
                ModMask::SHIFT,
                KeyAction::Custom(|wm| {
                    Command::new("sh")
                        .arg("-c")
                        .arg("maim -s | xclip -selection clipboard -t image/png -i")
                        .spawn()
                        .unwrap()
                        .wait()
                        .ok();

                    wm.draw_alert("Screenshot area copied to cliboard".to_string())
                        .ok();
                }),
            );
        }

        // === SUBMAP: NAV ===
        self.keybindings.add_submap("nav".to_string(), false);

        if let Some(key) = grabber.keysym_to_keycode(XK_J) {
            self.keybindings.bind_in_mode(
                "nav",
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    wm.focus_next().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_K) {
            self.keybindings.bind_in_mode(
                "nav",
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    wm.focus_prev().ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_ESCAPE) {
            self.keybindings
                .bind_in_mode("nav", key, ModMask::default(), KeyAction::ExitMode);
        }

        // === SUBMAP: APPS (oneshot) ===
        self.keybindings.add_submap("apps".to_string(), true);

        if let Some(key) = grabber.keysym_to_keycode(XK_T) {
            self.keybindings.bind_in_mode(
                "apps",
                key,
                ModMask::default(),
                KeyAction::Spawn("st".to_string()),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_E) {
            self.keybindings.bind_in_mode(
                "apps",
                key,
                ModMask::default(),
                KeyAction::Spawn("emacs".to_string()),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_F) {
            self.keybindings.bind_in_mode(
                "apps",
                key,
                ModMask::default(),
                KeyAction::Spawn("pcmanfm".to_string()),
            );
        }

        // === SUBMAP: ALERTS (oneshot) ===
        self.keybindings.add_submap("alerts".to_string(), true);

        if let Some(key) = grabber.keysym_to_keycode(XK_L) {
            self.keybindings.bind_in_mode(
                "alerts",
                key,
                ModMask::default(),
                KeyAction::Custom(|_| {
                    Command::new("dmenu_run")
                        .arg("-fn")
                        .arg("cozette")
                        .arg("-nb")
                        .arg("#000000")
                        .arg("-nf")
                        .arg("#ffffff")
                        .arg("-sb")
                        .arg("#0000ff")
                        .spawn()
                        .ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_B) {
            self.keybindings.bind_in_mode(
                "alerts",
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    let percentage =
                        get_command_output("cat /sys/class/power_supply/BAT0/capacity");
                    let bat_state = get_command_output("cat /sys/class/power_supply/BAT0/status");
                    let bat_state = bat_state.trim();

                    let label = match bat_state {
                        "Charging" => "CHR",
                        "Discharging" => "BAT",
                        "Full" => "FULL",
                        _ => "UNK",
                    };

                    wm.draw_alert(format!("{}: {}", label, percentage)).ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_A) {
            self.keybindings.bind_in_mode(
                "alerts",
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    let date = get_command_output("date '+%a %d %b %H:%M'");
                    wm.draw_alert(format!("{}", date)).ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_V) {
            self.keybindings.bind_in_mode(
                "alerts",
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    let percentage = get_command_output("pamixer --get-volume");
                    wm.draw_alert(format!("Vol: {}%", percentage)).ok();
                }),
            );
        }

        if let Some(key) = grabber.keysym_to_keycode(XK_D) {
            self.keybindings.bind_in_mode(
                "alerts",
                key,
                ModMask::default(),
                KeyAction::Custom(|wm| {
                    let date = get_command_output("date '+%a %d %b %H:%M'");
                    wm.draw_alert(format!("{}", date)).ok();
                }),
            );
        }

        self.update_grabs()?;
        Ok(())
    }

    fn update_grabs(&self) -> Result<()> {
        let setup = self.conn.setup();
        let screen = &setup.roots[self.screen_num];
        let grabber = KeyboardGrabber::new(&self.conn, screen, setup)?;

        grabber.ungrab_all_keys()?;

        let bindings = self.keybindings.active_bindings();

        for binding in bindings {
            if let Err(e) = grabber.grab_key(binding.keycode, binding.modifiers) {
                eprintln!("Failed to grab key: {}", e);
            }
        }

        self.conn.flush()?;
        Ok(())
    }

    fn execute_action(&mut self, action: KeyAction) -> Result<()> {
        match action {
            KeyAction::Spawn(cmd) => {
                println!("  ▶ Spawning: {}", cmd);
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .spawn()
                    .ok();
            }
            KeyAction::EnterMode(mode) => {
                self.draw_alert(format!("Mode: {}", mode))?;
                self.keybindings.enter_mode(mode);
                self.update_grabs()?;
            }
            KeyAction::ExitMode => {
                self.draw_alert(String::from("Mode: Normal"))?;
                self.keybindings.exit_mode();
                self.update_grabs()?;
            }
            KeyAction::CloseWindow => {
                if let Some(window) = self.focused_client() {
                    self.conn.kill_client(window)?;
                    self.conn.flush()?;
                }
            }
            KeyAction::FocusNext => {
                self.focus_next()?;
            }
            KeyAction::FocusPrev => {
                self.focus_prev()?;
            }
            KeyAction::Custom(func) => {
                func(self);
            }
            KeyAction::SwitchWorkspace(index) => {
                self.switch_to_workspace(index)?;
            }
            KeyAction::MoveToWorkspace(index) => {
                self.move_focused_to_workspace(index)?;
            }
            KeyAction::Quit => {
                exit(200);
            }
        }
        Ok(())
    }

    pub fn handle_key_press(&mut self, event: &KeyPressEvent) -> Result<()> {
        let keycode = event.detail;
        let modifiers = normalize_modifiers(ModMask::from(u16::from(event.state)));

        if let Some(action) = self.keybindings.find_action(keycode, modifiers) {
            let should_exit =
                self.keybindings.should_auto_exit() && !matches!(action, KeyAction::ExitMode);

            self.execute_action(action)?;

            if should_exit {
                self.keybindings.exit_mode();
                self.update_grabs()?;
            }
        }

        Ok(())
    }
}
