use std::io::{self, Stdout};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use ratatui::widgets::*;

fn parse_cmd(s: &str) -> Option<Vec<String>> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return None;
    }
    let v: Vec<String> = s.split_whitespace().map(String::from).collect();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

struct Config {
    phase1: Duration,
    phase2: Duration,
    auto_snooze: Duration,
    phase1_cmd: Option<Vec<String>>,
    phase2_cmd: Option<Vec<String>>,
}

fn parse_args() -> Result<Config, String> {
    let mut phase1_sec = 10u64;
    let mut phase2_sec = 300u64;
    let mut auto_snooze_sec = 60i64;
    let mut phase1_cmd_str = "";
    let mut phase2_cmd_str = "systemctl poweroff";
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].clone();
        let mut next = || {
            i += 1;
            if i < args.len() {
                args[i].as_str()
            } else {
                ""
            }
        };
        match arg.as_str() {
            "--phase1" | "-1" => {
                if let Ok(v) = next().parse::<u64>() {
                    if v > 0 {
                        phase1_sec = v;
                    }
                }
            }
            "--phase2" | "-2" => {
                if let Ok(v) = next().parse::<u64>() {
                    if v > 0 {
                        phase2_sec = v;
                    }
                }
            }
            "--auto-snooze" | "-a" => {
                if let Ok(v) = next().parse::<i64>() {
                    auto_snooze_sec = v;
                }
            }
            "--phase1-cmd" => phase1_cmd_str = next(),
            "--phase2-cmd" => phase2_cmd_str = next(),
            "-h" | "--help" => {
                eprintln!(
                    "usage: {} [options]\n  --phase1 SEC      idle before phase1 (default 10)\n  --phase2 SEC      idle before phase2 (default 300)\n  --auto-snooze SEC snooze after phase1 (default 60)\n  --phase1-cmd CMD  run on phase1 (default none)\n  --phase2-cmd CMD  run on phase2 (default systemctl poweroff); use 'none' to disable\n  -h, --help        show this",
                    args[0]
                );
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }
    if phase1_sec >= phase2_sec {
        return Err(format!("{}: phase1 must be less than phase2", args[0]));
    }
    let auto_snooze = if auto_snooze_sec < 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(auto_snooze_sec as u64)
    };
    Ok(Config {
        phase1: Duration::from_secs(phase1_sec),
        phase2: Duration::from_secs(phase2_sec),
        auto_snooze,
        phase1_cmd: parse_cmd(phase1_cmd_str),
        phase2_cmd: parse_cmd(phase2_cmd_str),
    })
}

fn format_dur(d: Duration) -> String {
    let total_ms = d.as_millis().min(u128::MAX) as i64;
    let total_ms = total_ms.max(0);
    let ms = (total_ms % 1000) as u32;
    let sec = ((total_ms / 1000) % 60) as u32;
    let min = (total_ms / 60_000) as u32;
    format!("{min}:{sec:02}.{ms:03}")
}

fn get_active_window_id() -> Option<String> {
    Command::new("xdotool")
        .args(["getactivewindow"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn close_window_by_id(window_id: &str) {
    let _ = Command::new("xdotool")
        .args(["windowclose", window_id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn run_phase1(cmd: Option<&[String]>, window_id: Option<&str>) {
    if let Some(id) = window_id {
        close_window_by_id(id);
    }
    let Some(cmd) = cmd else { return };
    if cmd.is_empty() {
        return;
    }
    let line = cmd.join(" ");
    let _ = Command::new("sh")
        .args(["-c", &line])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn run_phase2(cmd: &[String]) {
    if cmd.is_empty() {
        return;
    }
    let _ = Command::new(&cmd[0]).args(&cmd[1..]).status();
}

struct State {
    phase1_done: bool,
    snoozed_until: Option<Instant>,
    phase2_start: Option<Instant>,
    last_input: Instant,
    /// Last active window ID while user was active (so we close that window on P1, not our terminal).
    last_active_window_id: Option<String>,
}

fn run_phases(
    idle: Duration,
    now: Instant,
    cfg: &Config,
    state: &mut State,
) -> bool {
    if let Some(until) = state.snoozed_until {
        if now < until {
            return false;
        }
        state.snoozed_until = None;
        state.phase2_start = Some(now);
    }
    if let Some(start) = state.phase2_start {
        let phase2_elapsed = now.saturating_duration_since(start);
        if phase2_elapsed >= cfg.phase2 {
            if let Some(ref c) = cfg.phase2_cmd {
                run_phase2(c);
            }
            return true;
        }
    }
    if idle >= cfg.phase1 && !state.phase1_done {
        state.phase1_done = true;
        state.snoozed_until = Some(now + cfg.auto_snooze);
        let window_id = state.last_active_window_id.as_deref();
        run_phase1(cfg.phase1_cmd.as_deref(), window_id);
    }
    false
}

fn get_system_idle() -> Option<Duration> {
    user_idle2::UserIdle::get_time().ok().map(|u| u.duration())
}

/// When system idle is below this, treat as "user was active" and reset phases.
const ACTIVITY_THRESHOLD: Duration = Duration::from_secs(2);

fn phase_color(ratio: f64) -> Color {
    if ratio <= 0.2 {
        Color::Red
    } else if ratio <= 0.5 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn draw(frame: &mut Frame, cfg: &Config, state: &State, idle: Duration, now: Instant) {
    let title = Line::from(Span::styled("drastic-idle", Style::default().bold()));
    let idle_line = Line::from(Span::raw(format!("Idle {}", format_dur(idle))));
    let (p1_line, p2_line) = if let Some(until) = state.snoozed_until {
        let rem = until.saturating_duration_since(now);
        let p1 = Line::from(Span::styled(
            format!("Phase 1: close window ran (snoozed {})", format_dur(rem)),
            Style::default().fg(Color::Cyan),
        ));
        let p2 = Line::from(Span::styled(
            format!("Phase 2: starts in {} (after snooze)", format_dur(rem)),
            Style::default().fg(Color::DarkGray),
        ));
        (p1, p2)
    } else {
        let p1 = if !state.phase1_done && idle < cfg.phase1 {
            let left = cfg.phase1.saturating_sub(idle);
            let ratio = cfg.phase1.as_secs_f64().recip() * left.as_secs_f64();
            Line::from(Span::styled(
                format!("Phase 1: {} until close window", format_dur(left)),
                Style::default().fg(phase_color(ratio)),
            ))
        } else {
            Line::from(Span::styled("Phase 1: done", Style::default().fg(Color::Cyan)))
        };
        let p2 = match state.phase2_start {
            Some(start) => {
                let elapsed = now.saturating_duration_since(start);
                let p2_rem = cfg.phase2.saturating_sub(elapsed);
                let ratio = cfg.phase2.as_secs_f64().recip() * p2_rem.as_secs_f64();
                Line::from(Span::styled(
                    format!("Phase 2: {} until power off", format_dur(p2_rem)),
                    Style::default().fg(phase_color(ratio)),
                ))
            }
            None => Line::from(Span::styled(
                "Phase 2: after Phase 1",
                Style::default().fg(Color::DarkGray),
            )),
        };
        (p1, p2)
    };
    let body = Paragraph::new(vec![
        title,
        Line::from(""),
        idle_line,
        Line::from(""),
        p1_line,
        p2_line,
    ])
    .block(Block::default().padding(Padding::new(2, 2, 1, 1)));
    let footer = Paragraph::new(Line::from(Span::raw("q/ctrl+c quit")));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());
    frame.render_widget(body, chunks[0]);
    frame.render_widget(footer, chunks[1]);
}

fn run_tui(
    cfg: &Config,
    use_system_idle: bool,
    state: &mut State,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> io::Result<bool> {
    const TICK: Duration = Duration::from_millis(20);
    let now = Instant::now();
    let idle = if use_system_idle {
        get_system_idle().unwrap_or(Duration::ZERO)
    } else {
        now.saturating_duration_since(state.last_input)
    };
    if use_system_idle && idle < ACTIVITY_THRESHOLD {
        state.phase1_done = false;
        state.snoozed_until = None;
        state.phase2_start = None;
        if let Some(id) = get_active_window_id() {
            state.last_active_window_id = Some(id);
        }
    }
    if run_phases(idle, now, cfg, state) {
        return Ok(true);
    }
    terminal.draw(|f| draw(f, cfg, state, idle, now))?;
    if event::poll(TICK)? {
        match event::read()? {
            Event::Key(k) => {
                if k.kind == KeyEventKind::Press {
                    state.last_input = Instant::now();
                    state.phase1_done = false;
                    state.snoozed_until = None;
                    state.phase2_start = None;
                    if k.code == KeyCode::Char('q') || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL)) {
                        return Ok(true);
                    }
                }
            }
            Event::Mouse(_) => {
                state.last_input = Instant::now();
                state.phase1_done = false;
                state.snoozed_until = None;
                state.phase2_start = None;
            }
            _ => {}
        }
    }
    Ok(false) // continue loop
}

fn main() {
    let cfg = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let use_system_idle = get_system_idle().is_some();
    let mut state = State {
        phase1_done: false,
        snoozed_until: None,
        phase2_start: None,
        last_input: Instant::now(),
        last_active_window_id: get_active_window_id(),
    };
    enable_raw_mode().unwrap();
    crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture).unwrap();
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout())).unwrap();
    loop {
        match run_tui(&cfg, use_system_idle, &mut state, &mut terminal) {
            Ok(true) => break,
            Ok(false) => {}
            Err(e) => {
                crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture).unwrap();
                disable_raw_mode().unwrap();
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
    crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture).unwrap();
    disable_raw_mode().unwrap();
}
