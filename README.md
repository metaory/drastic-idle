# drastic-idle

Detect X11 idleness and run phased “drastic” actions: after a short idle, close the active window; after a longer idle, power off. All timeouts are configurable via CLI.

- **Phase 1**: idle ≥ phase1 (default 10s) → close active window.
- **Phase 2**: idle ≥ phase2 (default 5m) → `systemctl poweroff`.

Platform: X11 on Linux only.

**Why?** What if you pass out, or die, or can’t reach the machine? After a short idle the active window closes; after a longer idle it powers off, so the session doesn’t stay open when you’re not there to close it. Snooze (F12) when you’re still at the keyboard.

## Dependencies

- **Runtime**: X11 session (`DISPLAY` set).
- **Build**: gcc, libX11, libXss (e.g. Arch: `libx11`, `libxss`).

## Build

```bash
make
```

## Install (optional)

```bash
make install
```

Installs the binary to `$(PREFIX)/bin` (default `/usr/local/bin`). Override with `PREFIX=/usr` or `DESTDIR=...` for packaging.

## Run

```bash
./drastic-idle [options]
```

All time values are in **seconds**. Defaults:

| Option | Meaning | Default |
|--------|---------|--------|
| `--phase1 SEC` | Idle before closing window | 10 |
| `--phase2 SEC` | Idle before poweroff | 300 (5m) |
| `--auto-snooze SEC` | Snooze after phase 1 | 60 (1m) |
| `--manual-snooze SEC` | Snooze when F12 pressed | 300 (5m) |
| `--poll SEC` | Poll interval | 2 |
| `-h`, `--help` | Show usage | — |

Example: `./drastic-idle --phase1 15 --phase2 600`

Must run inside an X11 session. Poweroff may require polkit or elevated rights.

### Snooze

- **F12** — manual snooze for `--manual-snooze` seconds (no phase 1/2 during snooze).
- After phase 1 runs, the program auto-snoozes for `--auto-snooze` seconds.

### Timer

A small overlay at the top-right shows idle time, countdown to phase 1, and countdown to poweroff; when snoozed it shows remaining snooze time.

## License    
[MIT](LICENSE)