# dxwm

Minimal X11 window manager in Rust ([x11rb](https://github.com/psychon/x11rb)).
Configured by editing the source, like dwm.

## Running

```sh
make build          # target/release/dxwm
make test           # unit tests (layout, struts, text), no X server needed
make run-xephyr     # nested test session (scripts/xephyr.sh)
```

`~/.xinitrc`:

```sh
exec /path/to/dxwm >> ~/.dxwm.log 2>&1
```

On startup `~/.local/share/dxwm/autostart.sh` is run if it exists (not after a
restart).

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

## Main loop

`run` handles every pending event, then blocks in `poll(2)` on the X connection
until the next event or the nearest deadline of the timers: typing and hiding
alerts, the prompt's cursor blink and command output, a running screenshot and
the modeline status. With nothing pending it sleeps until an event arrives.

## Layout

Scrolling columns, like niri/PaperWM: the tiled clients of a workspace are
columns of a horizontal strip, in `Vec<Client>` order, each as tall as the
monitor (minus `MARGIN`).

- Every client has its own width as a fraction of the monitor (`column_width`).
  New ones use `DEFAULT_COLUMN_WIDTH` (0.5: two fit exactly side by side).
- `Workspace::scroll` is how far the strip is scrolled. Each layout moves it just
  enough to show the focused column completely, and never past the ends.
  `Super+C` centers the focused column once; `CENTER_FOCUSED_COLUMN = true`
  keeps it always centered.
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

- Position: a dialog is centered over its `WM_TRANSIENT_FOR` parent when the
  parent is on the current monitor. `USPosition` in `WM_NORMAL_HINTS` always
  wins, `PPosition` only for clients without a visible parent. Otherwise
  centered on the current monitor.
- Resizing with the mouse follows `WM_NORMAL_HINTS`: base size plus whole
  increments (terminal cells), within the min and max size.
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

## Rules

`RULES` in `config/mod.rs` changes how a new client is managed, matched by
`WM_CLASS` (instance or class, ignoring case):

```rust
Rule { workspace: Some(2), column_width: Some(1.0), ..Rule::class("firefox") },
Rule { floating: Some(true), ..Rule::class("pavucontrol") },
```

`workspace` opens it there without switching, `floating` forces floating or
tiled, `column_width` replaces `DEFAULT_COLUMN_WIDTH`. After a restart the saved
state wins over the rules.

## Scratchpad

`Super+`` ` toggles a floating terminal kept apart from the workspaces. If there
is none it runs `SCRATCHPAD_COMMAND`, and the window of class `SCRATCHPAD_CLASS`
becomes the scratchpad (`SCRATCHPAD_SIZE` of the monitor, centered). Toggling it
hides it when it is focused on the current workspace and brings it to the current
workspace and monitor otherwise. While hidden it is in no workspace
(`WindowManager::scratchpad`), with its frame unmapped. A restart shows it again.

## Overlays

Override-redirect windows (dunst notifications, menus, tooltips) are not managed:
they place themselves. dxwm only remembers the mapped ones (`overlays`) to raise
them again, together with its own alerts, every time it raises a client.

## Docks and EWMH

Windows of type `_NET_WM_WINDOW_TYPE_DOCK` (polybar, lemonbar...) are mapped but
not managed. Their `_NET_WM_STRUT_PARTIAL` (or `_NET_WM_STRUT`) is subtracted
from the monitors it touches: `Monitor::work_area()` is what the columns, the
alerts and the modeline use. Fullscreen and floating clients still use the whole
monitor.

Published on the root: `_NET_SUPPORTED`, `_NET_SUPPORTING_WM_CHECK`,
`_NET_CLIENT_LIST`, `_NET_ACTIVE_WINDOW`, `_NET_NUMBER_OF_DESKTOPS`,
`_NET_DESKTOP_NAMES`, `_NET_CURRENT_DESKTOP`, `_NET_WORKAREA` (the work area of
the current monitor for every desktop). On every client: `_NET_WM_DESKTOP`,
`_NET_FRAME_EXTENTS` (border and title, none in fullscreen), `_NET_WM_STATE_HIDDEN`
and the ICCCM `WM_STATE` (Iconic) while its workspace isn't visible. The
desktops are the workspaces of the current monitor. They are only written when
they change (`Ewmh` keeps the last values).

Client messages: `_NET_ACTIVE_WINDOW` jumps to the client (its monitor and
workspace), `_NET_CLOSE_WINDOW` closes it, `_NET_CURRENT_DESKTOP` switches
workspace, `_NET_WM_STATE` toggles fullscreen and `DEMANDS_ATTENTION`, and
`_NET_REQUEST_FRAME_EXTENTS` gets the extents before the window is mapped.

## Urgency

A client is urgent when it sets the urgency flag of `WM_HINTS` or asks for
`_NET_WM_STATE_DEMANDS_ATTENTION`, unless it has the focus. Its frame turns
inverse (`BORDER_URGENT`, `TITLE_*_URGENT`), its workspace gets a `*` in the
modeline and an alert shows where it is (`URGENT 3 title`). Getting the focus
clears it. dxwm keeps `DEMANDS_ATTENTION` in its `_NET_WM_STATE` while it lasts.

## Restart

`Super+Control+R` (or the `restart` command) re-executes dxwm keeping the clients:

1. The state is saved as properties: `_DXWM_CLIENT` on each client (monitor,
   workspace, position, column width, focused, floating geometry) and
   `_DXWM_MONITORS` on the root (current monitor and workspace of each).
2. dxwm returns from the main loop, closes its connection and `exec`s `argv[0]`
   with `--restart`, so a freshly built binary is the one that runs. The clients
   are in the save-set, so the server puts them back on the root.
3. On startup, `adopt_windows` manages every mapped window found on the root and
   restores the saved state. `--restart` skips the autostart and waits for the
   server to drop the old connection.

Windows left by another window manager are adopted the same way, without state.
A mapped window reparented into its frame sends an `UnmapNotify` to the root;
those are ignored, a client withdrawing is only seen through its frame.

## Modeline

A line in the style of Emacs' mode line at the bottom of each monitor, stacked
**under every window**: it is only seen where the desktop is (an empty workspace,
or the space a strip doesn't fill).

```
 1  ▮▮▯   2   3  ▬ ------------------------------------- BAT 82%  Mon 19 Oct 21:49
```

- The workspaces with clients and the visible one, which is in inverse video
  (gray on the monitors without the focus).
- After each number, a minimap of its strip: one block per column, scaled to
  `MINIMAP_VIEW` pixels per monitor, the focused one darker. The visible
  workspace has the part of the strip in view outlined.
- Battery and date on the right, refreshed every 5 s.

It's drawn again only when what it shows changes (`update_bars` compares with the
last one drawn). `SHOW_BAR = false` disables it.

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
- ICCCM focus models: the input focus is set unless `WM_HINTS` says `input =
  false`, and clients with `WM_TAKE_FOCUS` in `WM_PROTOCOLS` also get that message
  (globally active clients like Java only get the message).

## Keybindings

Normal mode grabs only the bound keys. A submap (`Super+A` apps, `Super+S` alerts)
grabs the whole keyboard; it is oneshot and any unbound key leaves it.

| Keys                    | Action                          |
|-------------------------|---------------------------------|
| `Super+Return`          | terminal                        |
| `` Super+` ``           | toggle the scratchpad           |
| `Super+J / K`           | focus next / previous client    |
| `Super+Shift+J / K`     | move column right / left        |
| `Super+H / L`           | narrower / wider column (10%)   |
| `Super+R`               | next preset width (1/3, 1/2, 2/3) |
| `Super+M`               | maximize column / default width |
| `Super+C`               | center the focused column       |
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
| `Super+Control+R`       | restart keeping the clients     |
| `Super+Shift+Escape`    | quit                            |
| `Super+A` → `t e f`     | terminal, editor, file manager  |
| `Super+S` → `b v d`     | battery, volume, date           |
| Media keys              | pamixer                         |
| `Print`                 | screenshot of the current monitor |
| `Shift+Print`           | screenshot of a selected area   |
| `Super+Print`           | screenshot of the focused client |

Alerts are a log used as feedback: one override-redirect window in the bottom
right corner of the current monitor, `MARGIN` away from its edges like the
clients. Each alert is a line with the time and its tag (`22:11:23 WS     2`);
the newest one is typed in the title bar colors and the previous ones
(`ALERT_LINES`, at most `ALERT_HISTORY` seconds old) are dimmed above it. The
window hides 3 s after the last alert, unless a submap is active, and the lines
are kept for the next one.

## Prompt

An input line with a list below, filtered by what is typed (every word has to
appear, ignoring case). It is an override-redirect window drawn with the font and
colors of the decorations, and holds an active keyboard grab while it is open, so
the bindings don't run. The input line reads like a shell prompt
(`user@host:run$`, `out`, `win`, `cmd`) with a blinking block cursor, the counter
is `[selected/total]`, and the characters that matched are underlined.

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
| `scratchpad.rs`          | showing and hiding the scratchpad                       |
| `rules.rs`               | matching `RULES` by `WM_CLASS`                          |
| `floating.rs`            | dialog detection, configure requests, mouse move/resize |
| `workspaces.rs`          | `Workspace`, `WorkspaceManager`, switching/moving       |
| `monitors.rs`            | RandR detection, refresh, monitor focus/move, pointer   |
| `alerts.rs`              | alert log window                                        |
| `bar.rs`                 | modeline with the workspace minimaps                    |
| `ewmh.rs`                | EWMH properties and messages, docks and struts          |
| `restart.rs`             | saving the state, re-exec and adopting windows          |
| `prompt/mod.rs`          | prompt: opening it, keys, running the selected entry    |
| `prompt/input.rs`        | input line editing, filtering and selection             |
| `prompt/draw.rs`         | drawing the prompt                                      |
| `prompt/output.rs`       | command output followed live                            |
| `prompt/launcher.rs`     | programs in `$PATH` and the history                     |
| `screenshot.rs`          | background screenshots                                  |
| `keybindings.rs`         | binding tables and modes                                |
| `keyboard.rs`            | keymap and key grabs                                    |
| `keysyms.rs`             | keysym constants                                        |
| `config/mod.rs`          | colors, sizes, fonts, apps                              |
| `config/keybinds.rs`     | the bindings and how actions run                        |
| `atoms.rs`, `utils.rs`   | X atoms, autostart and shell helpers                    |

## Roadmap

- [ ] Columns: stack several clients in one column
      (niri's consume/expel) and resize columns with the mouse.
