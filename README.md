# drastic-idle

TUI that tracks idle time and runs phased actions: after phase1 (optional command + close focused window on X11), after phase2 (power off). Phase1 = 10s idle, phase2 = 5m countdown then `systemctl poweroff`.

- **Phase 1**: idle ≥ 10s → close the window that had focus (X11 only), run optional phase1 command (none by default); then phase 2 starts.
- **Phase 2**: 5m countdown → `systemctl poweroff` then exit.

## Support

| Feature | X11 | Wayland |
|--------|-----|---------|
| **System-wide idle** | ✓ (X11 backend) | ✓ on GNOME (Mutter/DBus via user-idle2) |
| **Phase 1: close focused window** | ✓ (needs `xdotool`) | ✗ |

**Why no “close window” on Wayland:** That behavior uses `xdotool` (get active window, then close it). Wayland has no equivalent: the protocol does not let one app query or control other apps’ windows for security. So on Wayland the app still runs (idle + phase2 countdown work), but phase1 does not close any window.

If no system idle backend is available, idle is TUI-only (only input in the app resets the timer).

## Requirements

- **Build:** Rust (e.g. Arch: `rust`).
- **Run:** Terminal with alt-screen and mouse. Optional: `xdotool` for phase1 “close window” on X11. Poweroff may need polkit or root.

## Build & install

```bash
make          # → target/release/drastic-idle
make install  # → $(PREFIX)/bin (default /usr/local/bin)
```

Or `cargo build --release`.

## Usage

```bash
./drastic-idle
```

- **q** or **Ctrl+c** — quit.
- Idle is shown with ms; any key/mouse **system-wide** (when system idle is used) resets it.

Phase 1 on X11: the window that had focus when you went idle is closed (via `xdotool`). No extra phase1 command by default.

## License

[MIT](LICENSE)
