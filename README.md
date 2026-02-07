# drastic-idle

Detect X11 idleness and run phased “drastic” actions: after a short idle, close the active window; after a longer idle, power off. All timeouts are configurable via CLI.

- **Phase 1**: idle ≥ phase1 (default 10s) → close active window.
- **Phase 2**: idle ≥ phase2 (default 5m) → `systemctl poweroff`.

Platform: X11 on Linux only.

**Why?** What if you pass out, or die, or can’t reach the machine? After a short idle the active window closes; after a longer idle it powers off, so the session doesn’t stay open. Every keyboard or mouse action resets the idle count.

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
| `--poll SEC` | Poll interval | 2 |
| `-h`, `--help` | Show usage | — |

Example: `./drastic-idle --phase1 15 --phase2 600`

Must run inside an X11 session. Poweroff may require polkit or elevated rights.

After phase 1 runs, the program auto-snoozes for `--auto-snooze` seconds (no phase 2 during that time).

### Timer

A small overlay at the top-right shows idle time, countdown to phase 1, and countdown to poweroff (with tenths of a second); when auto-snoozed it shows remaining snooze time. Every keyboard or mouse action resets the idle count.

## License    
[MIT](LICENSE)