# drastic-idle

TUI that tracks idle time and runs phased actions: after phase1 (optional command), after phase2 (default: power off). All timeouts are configurable via CLI.

- **Phase 1**: idle ≥ phase1 (default 10s) → run optional `--phase1-cmd`; then auto-snooze.
- **Phase 2**: idle ≥ phase2 (default 5m) → run `--phase2-cmd` (default `systemctl poweroff`) then exit.

**Idle source**: When available, idle is **system-wide** (X11, or Wayland/GNOME via DBus). Any keyboard or mouse activity in any window resets the timer. If no system idle backend is available, idle is TUI-local (only input in the app resets it). The counter shows milliseconds and updates smoothly (~50 fps).

## Dependencies

- **Build**: Rust (e.g. Arch: `rust`).
- **Runtime**: terminal with alt-screen and mouse support.

## Build

```bash
make
```

Or:

```bash
cargo build --release
```

Binary: `target/release/drastic-idle`.

## Install (optional)

```bash
make install
```

Installs the binary to `$(PREFIX)/bin` (default `/usr/local/bin`).

## Run

```bash
./drastic-idle [options]
```

All time values are in **seconds**. Defaults:

| Option | Meaning | Default |
|--------|---------|---------|
| `--phase1 SEC` | Idle before phase1 | 10 |
| `--phase2 SEC` | Idle before phase2 | 300 (5m) |
| `--auto-snooze SEC` | Snooze after phase1 | 60 (1m) |
| `--phase1-cmd CMD` | Command on phase1 (space-separated) | none |
| `--phase2-cmd CMD` | Command on phase2; use `none` to disable | systemctl poweroff |
| `-h`, `--help` | Show usage | — |

Example: `./drastic-idle --phase1 15 --phase2 600`

After phase1 runs, the program auto-snoozes for `--auto-snooze` seconds (phase2 is not checked during snooze). Poweroff may require polkit or elevated rights.

### Phase 1: close window

Phase 1 often runs a command to close the active (or focused) window. The command runs with your environment (DISPLAY/WAYLAND_DISPLAY). Use a command that works on your session:

- **X11**: `xdotool getactivewindow windowclose` or `wmctrl -c :ACTIVE:` (needs `xdotool` / `wmctrl`).
- **Wayland**: `xdotool` does not work. Use a compositor-specific method or e.g. `ydotool` if your compositor supports it; otherwise use a small script that talks to your compositor’s DBus/API.

If nothing happens, try running the same command in a terminal (without drastic-idle) to confirm it works.

### Timer

The TUI shows idle time with milliseconds, phase1 status (countdown or "done"), and countdown to phase2 (or remaining snooze time). Any key or mouse action **anywhere** (when system idle is used) resets the idle count; otherwise only input in the TUI does. Press **q** or **Ctrl+c** to quit.

## License

[MIT](LICENSE)
