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

Scrolling columns, like niri/PaperWM: the tiled clients of a workspace are
columns of a horizontal strip, in `Vec<Client>` order, each as tall as the
monitor (minus `MARGIN`).

- Every client has its own width as a fraction of the monitor (`column_width`).
  New ones use `DEFAULT_COLUMN_WIDTH` (0.5: two fit exactly side by side).
- `Workspace::scroll` is how far the strip is scrolled. Each layout moves it just
  enough to show the focused column completely, and never past the ends.
- A new client opens right after the focused one.
- Columns out of view stay mapped but parked off-screen (`x = -2 * monitor
  width`), so scrolling is instant. A column cut by the monitor edge is clipped
  with the Shape extension, so it never shows on the next monitor.
- Hovering a partially visible column focuses it without scrolling; the keyboard
  and `Super+wheel` scroll.

Fullscreen clients cover the whole monitor without border nor title bar.

Switching workspaces unmaps the clients' frames instead. The client stays mapped
inside its frame, so no `UnmapNotify` reaches us that could be mistaken for the
client closing.

## Floating clients

Dialogs float instead of being tiled: windows with `WM_TRANSIENT_FOR`, a
`_NET_WM_WINDOW_TYPE` of dialog/utility/splash/notification, or a fixed size
(min == max in `WM_NORMAL_HINTS`). They keep the size they asked for, stay above
tiled and fullscreen clients and can't leave their monitor.

- Position: the one the client asked for if it set `USPosition`/`PPosition` in
  `WM_NORMAL_HINTS`, otherwise centered on the current monitor.
- Notification windows don't take the focus when they open.

- A floating client is inserted right after the focused one. The column kept
  in view while a dialog is focused is the closest tiled client before it, and
  closing the dialog gives the focus back to it.
- All floating clients of a workspace are visible, wherever the strip is scrolled.
- `Super+Shift+Space` toggles floating. `Super+Button1` moves and `Super+Button3`
  resizes the floating client under the pointer.
- `ConfigureRequest`: unmanaged windows get what they ask for, floating clients
  can move and resize themselves, tiled ones get a synthetic `ConfigureNotify` with their
  real geometry.

## Overlays

Override-redirect windows (dunst notifications, menus, tooltips) are not managed:
they place themselves. dxwm only remembers the mapped ones (`overlays`) to raise
them again, together with its own alerts, every time it raises a client.

## Decorations

Every client is reparented into a frame window (like herbstluftwm): the frame
has the border (`BORDER_WIDTH`) and a title bar on top (`TITLE_HEIGHT`) with the
client's `_NET_WM_NAME`/`WM_NAME` on the left, the client fills the rest. Colors
are `BORDER_*` and `TITLE_BG_*`/`TITLE_FG_*` (focused / unfocused) in
`config/mod.rs`; `TITLE_HEIGHT = 0` leaves only the border.

- Titles use an X core font (`TITLE_FONT`, falls back to `fixed`), no Xft.
  Titles that don't fit are cut with `...`.
- The frame selects `SubstructureRedirect`/`SubstructureNotify`, so the client's
  map requests and unmaps arrive with the frame as parent. Layout, stacking,
  mapping and `EnterNotify` work on frames; focus and the workspace lists use the
  client window.
- Clients are added to the save-set: if dxwm dies they go back to the root.
  A client that withdraws is reparented to the root and its frame destroyed.

## Focus

- Focus follows the mouse (`EnterNotify`). Pointer motion over an empty monitor
  focuses that monitor.
- Every layout/warp sets `enter_barrier`; `EnterNotify` events generated before it
  are ignored, so windows moving under a still pointer don't steal the focus.
- Only the focused client of the current monitor gets the focused colors.

## Keybindings

Normal mode grabs only the bound keys. A submap (`Super+A` apps, `Super+S` alerts)
grabs the whole keyboard; it is oneshot and any unbound key leaves it.

| Keys                    | Action                          |
|-------------------------|---------------------------------|
| `Super+Return`          | terminal                        |
| `Super+J / K`           | focus next / previous client    |
| `Super+Shift+J / K`     | move column right / left        |
| `Super+H / L`           | narrower / wider column (10%)   |
| `Super+R`               | next preset width (1/3, 1/2, 2/3) |
| `Super+M`               | maximize column / default width |
| `Super+wheel`           | focus previous / next client    |
| `Super+F`               | toggle fullscreen               |
| `Super+Shift+Space`     | toggle floating                 |
| `Super+Button1 / 3`     | move / resize floating client   |
| `Super+Shift+C`         | close client                    |
| `Super+1..9`            | switch workspace                |
| `Super+Shift+1..9`      | move client to workspace        |
| `Super+Tab`             | last workspace                  |
| `Super+, / .`           | focus previous / next monitor   |
| `Super+Shift+, / .`     | move client to monitor          |
| `Super+Shift+R`         | re-read monitors                |
| `Super+B`               | move pointer out of the way     |
| `Super+G`               | clear alerts                    |
| `Super+P`               | prompt: run a program           |
| `Super+Shift+P`         | prompt: run a command and show its output |
| `Super+W`               | prompt: jump to a client        |
| `Super+;`               | prompt: WM commands (`COMMANDS`) |
| `Super+Shift+Escape`    | quit                            |
| `Super+A` → `t e f`     | terminal, editor, file manager  |
| `Super+S` → `b v d`     | battery, volume, date           |
| Media keys              | pamixer                         |
| `Print`                 | screenshot of the current monitor |
| `Shift+Print`           | screenshot of a selected area   |
| `Super+Print`           | screenshot of the focused client |

Alerts are small override-redirect windows (bottom right, 3 s) used as feedback.

## Prompt

An input line with a list below, filtered by what is typed (every word has to
appear, ignoring case). It is an override-redirect window drawn with the font and
colors of the decorations, and holds an active keyboard grab while it is open, so
the bindings don't run.

- `Super+P` lists the commands run before (`~/.local/share/dxwm/history`) and the
  programs in `$PATH`. `Return` runs the selected entry, `Shift+Return` what was
  typed, `Control+Return` runs it showing its output.
- `Super+Shift+P` is the same launcher but `Return` shows the output: the command
  runs in the background with stdout and stderr in a temporary file that the main
  loop follows, so the prompt shows it live and the WM never waits. `j/k`, the
  arrows, `PageUp/PageDown`, `Space`, `Home/End` scroll; `Escape`, `Return` or `q`
  close it, killing the command (its whole process group) if it is still running.
- `Super+W` lists the clients of every monitor and workspace, `Super+;` the
  `COMMANDS` of `config/keybinds.rs`.
- Editing keys are the readline ones (`C-a C-e C-b C-f C-h C-d C-w C-u C-k`),
  `Tab` completes with the selected entry, `C-n/C-p` or the arrows move the
  selection, `Escape` or `C-g` cancel.

Screenshots (maim) are saved to `SCREENSHOT_DIR` (`~/Pictures/Screenshots`) and
copied to the clipboard (xclip). They run in the background and the main loop
shows an alert when they finish; the WM never waits for them.

## Source map

| File                     | Contents                                                |
|--------------------------|---------------------------------------------------------|
| `main.rs`                | entry point                                             |
| `wm.rs`                  | `WindowManager`, setup, event loop and dispatch         |
| `clients.rs`             | manage/unmanage, focus, swap, close, fullscreen         |
| `layout.rs`              | scrolling columns, column widths                        |
| `decorations.rs`         | frames, title bar drawing                               |
| `floating.rs`            | dialog detection, configure requests, mouse move/resize |
| `workspaces.rs`          | `Workspace`, `WorkspaceManager`, switching/moving       |
| `monitors.rs`            | RandR detection, refresh, monitor focus/move, pointer   |
| `alerts.rs`              | alert windows                                           |
| `prompt.rs`              | prompt: launcher, command output, clients, commands     |
| `screenshot.rs`          | background screenshots                                  |
| `keybindings.rs`         | binding tables and modes                                |
| `keyboard.rs`            | keymap and key grabs                                    |
| `keysyms.rs`             | keysym constants                                        |
| `config/mod.rs`          | colors, sizes, fonts, apps                              |
| `config/keybinds.rs`     | the bindings and how actions run                        |
| `atoms.rs`, `utils.rs`   | X atoms, autostart and shell helpers                    |

## Roadmap

- [ ] Columns: center the focused column, stack several clients in one column
      (niri's consume/expel) and resize columns with the mouse.
- [ ] Manage windows that already exist on startup, so restarting dxwm doesn't lose them.
- [ ] Center dialogs over their `WM_TRANSIENT_FOR` parent and respect size hints
      (min/max, increments) when resizing floating clients.
- [ ] Respect `WM_HINTS` input and `WM_TAKE_FOCUS` (some toolkits need it to get focus).
- [ ] Basic EWMH for bars and tools: `_NET_SUPPORTED`, `_NET_ACTIVE_WINDOW`,
      `_NET_CLIENT_LIST`, `_NET_CURRENT_DESKTOP`.
- [ ] Block on the X connection instead of polling every 32 ms (wake up only for
      events or alert expiry).
- [ ] Restart in place (re-exec) keeping the clients.
- [ ] Urgency hint in the border or as an alert.
