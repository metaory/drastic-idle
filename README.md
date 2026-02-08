# drastic-idle

TUI that tracks idle time and runs phased actions: after phase1 (optional command), after phase2 (power off). Timeouts and commands are fixed (phase1 10s, phase2 300s, phase2 runs `systemctl poweroff`). Config file or env may be added later.

- **Phase 1**: idle ≥ 10s → run optional phase1 command (none by default); then phase 2 starts.
- **Phase 2**: 5m countdown after phase 1 → run `systemctl poweroff` then exit.

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
./drastic-idle
```

Phase 2 starts immediately after phase 1. Poweroff may require polkit or elevated rights.

### Phase 1: close window

On **X11**, when phase 1 runs the app closes the **window that had focus when you went idle** (using `xdotool getactivewindow` while you’re active, then `xdotool windowclose <id>` on phase 1). So the window that was in use (e.g. browser) is closed, not the terminal running drastic-idle. Requires `xdotool` to be installed.

No extra phase1 command runs by default. A config file or env could add one later.

**Wayland**: `xdotool` does not work. Phase 1 only closes the last window on X11; there is no built-in “close last window” on Wayland.

### Timer

The TUI shows idle time with milliseconds, phase1 status (countdown or "done"), and countdown to phase2. Any key or mouse action **anywhere** (when system idle is used) resets the idle count; otherwise only input in the TUI does. Press **q** or **Ctrl+c** to quit.

## License

[MIT](LICENSE)
