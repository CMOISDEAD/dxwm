# dxwm

Minimal X11 window manager in Rust ([x11rb](https://github.com/psychon/x11rb)).
Configured by editing the source, like dwm.

## Running

```sh
make build          # target/release/dxwm
make run-xephyr     # nested test session (scripts/xephyr.sh)
```

`~/.xinitrc`:

```sh
exec /path/to/dxwm >> ~/.dxwm.log 2>&1
```

On startup `~/.local/share/dxwm/autostart.sh` is run if it exists.

**Monitors are not configured by dxwm.** Enable and place outputs yourself, e.g.
in `autostart.sh`:

```sh
xrandr --output HDMI-1-1 --auto --right-of eDP-1
```

dxwm listens to RandR events and picks up the change on its own. `Super+Shift+R`
forces a re-read (needed for `xrandr --setmonitor`, which sends no events).

## Model

```
MonitorManager ── Monitor (one per RandR monitor, left to right)
                    └── WorkspaceManager (9 workspaces per monitor)
                          └── Workspace ── Vec<Client> (layout order) + focused client
```

- Each monitor has its own workspaces, `Super+1..9` acts on the current monitor.
- When a monitor disappears, its clients move to the same workspace of the primary one.
- Clients are matched to a monitor by name on refresh, so workspaces survive resolution changes.

## Layout

One client at a time: the focused client fills the monitor (minus `MARGIN`),
the others stay mapped but parked off-screen (`x = -2 * monitor width`), so
switching with `Super+J/K` is instant. Fullscreen clients cover the whole
monitor without border.

Switching workspaces unmaps the clients instead. `pending_unmaps` remembers those
unmaps so the resulting `UnmapNotify` is not mistaken for the client closing.

## Focus

- Focus follows the mouse (`EnterNotify`). Pointer motion over an empty monitor
  focuses that monitor.
- Every layout/warp sets `enter_barrier`; `EnterNotify` events generated before it
  are ignored, so windows moving under a still pointer don't steal the focus.
- Only the focused client of the current monitor gets `BORDER_FOCUSED`.

## Keybindings

Normal mode grabs only the bound keys. A submap (`Super+A` apps, `Super+S` alerts)
grabs the whole keyboard; it is oneshot and any unbound key leaves it.

| Keys                    | Action                          |
|-------------------------|---------------------------------|
| `Super+Return`          | terminal                        |
| `Super+J / K`           | focus next / previous client    |
| `Super+Shift+J / K`     | swap with next / previous       |
| `Super+F`               | toggle fullscreen               |
| `Super+Shift+C`         | close client                    |
| `Super+1..9`            | switch workspace                |
| `Super+Shift+1..9`      | move client to workspace        |
| `Super+Tab`             | last workspace                  |
| `Super+, / .`           | focus previous / next monitor   |
| `Super+Shift+, / .`     | move client to monitor          |
| `Super+Shift+R`         | re-read monitors                |
| `Super+B`               | move pointer out of the way     |
| `Super+G`               | clear alerts                    |
| `Super+Shift+Escape`    | quit                            |
| `Super+A` → `t e f`     | terminal, editor, file manager  |
| `Super+S` → `l b v d`   | dmenu, battery, volume, date    |
| Media keys, `Print`     | pamixer, maim                   |

Alerts are small override-redirect windows (bottom right, 3 s) used as feedback.

## Source map

| File                     | Contents                                              |
|--------------------------|-------------------------------------------------------|
| `main.rs`                | entry point                                           |
| `wm.rs`                  | `WindowManager`, setup, event loop and dispatch       |
| `clients.rs`             | manage/unmanage, focus, swap, close, fullscreen       |
| `layout.rs`              | client placement                                      |
| `workspaces.rs`          | `Workspace`, `WorkspaceManager`, switching/moving     |
| `monitors.rs`            | RandR detection, refresh, monitor focus/move, pointer |
| `alerts.rs`              | alert windows                                         |
| `keybindings.rs`         | binding tables and modes                              |
| `keyboard.rs`            | keymap and key grabs                                  |
| `keysyms.rs`             | keysym constants                                      |
| `config/mod.rs`          | colors, sizes, apps                                   |
| `config/keybinds.rs`     | the bindings and how actions run                      |
| `atoms.rs`, `utils.rs`   | X atoms, autostart and shell helpers                  |

## Roadmap

- [ ] Answer `ConfigureRequest` (send the current geometry back). Some apps wait for it.
- [ ] Manage windows that already exist on startup, so restarting dxwm doesn't lose them.
- [ ] Floating clients: dialogs / `WM_TRANSIENT_FOR` / `_NET_WM_WINDOW_TYPE_DIALOG`
      are tiled today. Moving and resizing with the mouse.
- [ ] Respect `WM_HINTS` input and `WM_TAKE_FOCUS` (some toolkits need it to get focus).
- [ ] Basic EWMH for bars and tools: `_NET_SUPPORTED`, `_NET_ACTIVE_WINDOW`,
      `_NET_CLIENT_LIST`, `_NET_CURRENT_DESKTOP`.
- [ ] Block on the X connection instead of polling every 32 ms (wake up only for
      events or alert expiry).
- [ ] Restart in place (re-exec) keeping the clients.
- [ ] Urgency hint in the border or as an alert.
