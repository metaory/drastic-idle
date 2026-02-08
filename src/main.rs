use std::io::{self, Stdout};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use ratatui::widgets::*;

// --- Config ---

struct Config {
    phase1_secs: u64,
    phase2_secs: u64,
    auto_snooze_secs: u64,
    phase1_cmd: Option<Vec<String>>,
    phase2_cmd: Option<Vec<String>>,
}

impl Config {
    fn phase1(&self) -> Duration {
        Duration::from_secs(self.phase1_secs)
    }
    fn phase2(&self) -> Duration {
        Duration::from_secs(self.phase2_secs)
    }
    fn auto_snooze(&self) -> Duration {
        Duration::from_secs(self.auto_snooze_secs)
    }
}

fn parse_opt_value(s: &str) -> Option<Vec<String>> {
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

/// Parse argv into (key, value) pairs. Key is the option; value is from `--key=val` or next arg.
fn argv_pairs(args: &[String]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        let (key, mut val) = if let Some(eq) = arg.find('=') {
            let k = arg[..eq].to_string();
            let v = arg[eq + 1..].trim().to_string();
            (k, if v.is_empty() { None } else { Some(v) })
        } else {
            (arg.clone(), None)
        };
        let takes_val = matches!(
            key.as_str(),
            "--phase1" | "-1" | "--phase2" | "-2" | "--auto-snooze" | "-a" | "--phase1-cmd" | "--phase2-cmd"
        );
        if val.is_none() && takes_val {
            i += 1;
            if i < args.len() {
                val = Some(args[i].clone());
            }
        }
        out.push((key, val));
        i += 1;
    }
    out
}

fn parse_args() -> Result<Config, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut phase1_secs = 10u64;
    let mut phase2_secs = 300u64;
    let mut auto_snooze_secs = 60u64;
    let mut phase1_cmd_str = String::new();
    let mut phase2_cmd_str = String::from("systemctl poweroff");

    for (key, val) in argv_pairs(&args) {
        match key.as_str() {
            "-h" | "--help" => {
                eprintln!(
                    "usage: {} [options]\n  --phase1 SEC      idle before phase1 (default 10)\n  --phase2 SEC      idle before phase2 (default 300)\n  --auto-snooze SEC snooze after phase1 (default 60)\n  --phase1-cmd CMD  run on phase1 (default none)\n  --phase2-cmd CMD  run on phase2 (default systemctl poweroff); use 'none' to disable\n  -h, --help        show this",
                    args[0]
                );
                std::process::exit(0);
            }
            k if k == "--phase1" || k == "-1" => {
                if let Some(s) = val {
                    if let Ok(n) = s.parse::<u64>() {
                        if n > 0 {
                            phase1_secs = n;
                        }
                    }
                }
            }
            k if k == "--phase2" || k == "-2" => {
                if let Some(s) = val {
                    if let Ok(n) = s.parse::<u64>() {
                        if n > 0 {
                            phase2_secs = n;
                        }
                    }
                }
            }
            "--auto-snooze" | "-a" => {
                if let Some(s) = val {
                    if let Ok(n) = s.parse::<i64>() {
                        auto_snooze_secs = n.max(0) as u64;
                    }
                }
            }
            "--phase1-cmd" => phase1_cmd_str = val.unwrap_or_default(),
            "--phase2-cmd" => phase2_cmd_str = val.unwrap_or_default(),
            _ => {}
        }
    }

    if phase1_secs >= phase2_secs {
        return Err(format!("{}: phase1 must be less than phase2", args[0]));
    }
    Ok(Config {
        phase1_secs,
        phase2_secs,
        auto_snooze_secs,
        phase1_cmd: parse_opt_value(&phase1_cmd_str),
        phase2_cmd: parse_opt_value(&phase2_cmd_str),
    })
}

// --- Helpers ---

fn format_dur(d: Duration) -> String {
    let total_ms = d.as_millis().min(u128::MAX) as i64;
    let total_ms = total_ms.max(0);
    let ms = (total_ms % 1000) as u32;
    let sec = ((total_ms / 1000) % 60) as u32;
    let min = (total_ms / 60_000) as u32;
    format!("{min}:{sec:02}.{ms:03}")
}

fn phase_color(ratio: f64) -> Color {
    if ratio <= 0.2 {
        Color::Red
    } else if ratio <= 0.5 {
        Color::Yellow
    } else {
        Color::Green
    }
}

const ACTIVITY_THRESHOLD: Duration = Duration::from_secs(2);

// --- Idle / X11 ---

fn system_idle() -> Option<Duration> {
    user_idle2::UserIdle::get_time().ok().map(|u| u.duration())
}

fn active_window_id() -> Option<String> {
    Command::new("xdotool")
        .args(["getactivewindow"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn close_window(id: &str) {
    let _ = Command::new("xdotool")
        .args(["windowclose", id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn run_cmd_sh(line: &str) {
    let _ = Command::new("sh")
        .args(["-c", line])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn run_cmd_argv(cmd: &[String]) {
    if cmd.is_empty() {
        return;
    }
    let _ = Command::new(&cmd[0]).args(&cmd[1..]).status();
}

// --- State ---

struct AppState {
    phase1_done: bool,
    snoozed_until: Option<Instant>,
    phase2_start: Option<Instant>,
    last_input: Instant,
    last_active_window_id: Option<String>,
}

impl AppState {
    fn new() -> Self {
        Self {
            phase1_done: false,
            snoozed_until: None,
            phase2_start: None,
            last_input: Instant::now(),
            last_active_window_id: active_window_id(),
        }
    }

    fn reset_phases(&mut self) {
        self.phase1_done = false;
        self.snoozed_until = None;
        self.phase2_start = None;
    }

    fn tick_idle(&mut self, use_system: bool, idle: Duration) {
        if !use_system || idle >= ACTIVITY_THRESHOLD {
            return;
        }
        self.reset_phases();
        if let Some(id) = active_window_id() {
            self.last_active_window_id = Some(id);
        }
    }
}

// --- Phase logic ---

/// Returns true if app should exit (phase2 ran).
fn tick_phases(cfg: &Config, state: &mut AppState, idle: Duration, now: Instant) -> bool {
    if let Some(until) = state.snoozed_until {
        if now < until {
            return false;
        }
        state.snoozed_until = None;
        state.phase2_start = Some(now);
    }

    if let Some(start) = state.phase2_start {
        let elapsed = now.saturating_duration_since(start);
        if elapsed >= cfg.phase2() {
            if let Some(ref c) = cfg.phase2_cmd {
                run_cmd_argv(c);
            }
            return true;
        }
    }

    if idle >= cfg.phase1() && !state.phase1_done {
        state.phase1_done = true;
        state.snoozed_until = Some(now + cfg.auto_snooze());
        if let Some(id) = state.last_active_window_id.as_deref() {
            close_window(id);
        }
        if let Some(ref c) = cfg.phase1_cmd {
            run_cmd_sh(&c.join(" "));
        }
    }
    false
}

// --- UI ---

fn render(cfg: &Config, state: &AppState, idle: Duration, now: Instant) -> Vec<Line<'static>> {
    let title = Line::from(Span::styled("drastic-idle", Style::default().bold()));
    let idle_line = Line::from(format!("Idle {}", format_dur(idle)));

    let (p1, p2) = if let Some(until) = state.snoozed_until {
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
        let p1 = if !state.phase1_done && idle < cfg.phase1() {
            let left = cfg.phase1().saturating_sub(idle);
            let r = left.as_secs_f64() / cfg.phase1().as_secs_f64();
            Line::from(Span::styled(
                format!("Phase 1: {} until close window", format_dur(left)),
                Style::default().fg(phase_color(r)),
            ))
        } else {
            Line::from(Span::styled("Phase 1: done", Style::default().fg(Color::Cyan)))
        };
        let p2 = match state.phase2_start {
            Some(start) => {
                let elapsed = now.saturating_duration_since(start);
                let rem = cfg.phase2().saturating_sub(elapsed);
                let r = rem.as_secs_f64() / cfg.phase2().as_secs_f64();
                Line::from(Span::styled(
                    format!("Phase 2: {} until power off", format_dur(rem)),
                    Style::default().fg(phase_color(r)),
                ))
            }
            None => Line::from(Span::styled(
                "Phase 2: after Phase 1",
                Style::default().fg(Color::DarkGray),
            )),
        };
        (p1, p2)
    };

    vec![
        title,
        Line::from(""),
        idle_line,
        Line::from(""),
        p1,
        p2,
    ]
}

fn draw_ui(frame: &mut Frame, cfg: &Config, state: &AppState, idle: Duration, now: Instant) {
    let body = Paragraph::new(render(cfg, state, idle, now))
        .block(Block::default().padding(Padding::new(2, 2, 1, 1)));
    let footer = Paragraph::new(Line::from("q/ctrl+c quit"));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());
    frame.render_widget(body, chunks[0]);
    frame.render_widget(footer, chunks[1]);
}

// --- Main loop ---

fn run_tui(
    cfg: &Config,
    use_system_idle: bool,
    state: &mut AppState,
    term: &mut Terminal<CrosstermBackend<Stdout>>,
) -> io::Result<bool> {
    let now = Instant::now();
    let idle = use_system_idle
        .then(system_idle)
        .flatten()
        .unwrap_or_else(|| now.saturating_duration_since(state.last_input));

    state.tick_idle(use_system_idle, idle);
    if tick_phases(cfg, state, idle, now) {
        return Ok(true);
    }

    term.draw(|f| draw_ui(f, cfg, state, idle, now))?;

    const TICK: Duration = Duration::from_millis(20);
    if event::poll(TICK)? {
        match event::read()? {
            Event::Key(k) => {
                if k.kind == KeyEventKind::Press {
                    state.last_input = Instant::now();
                    state.reset_phases();
                    if k.code == KeyCode::Char('q')
                        || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
                    {
                        return Ok(true);
                    }
                }
            }
            Event::Mouse(_) => {
                state.last_input = Instant::now();
                state.reset_phases();
            }
            _ => {}
        }
    }
    Ok(false)
}

fn main() {
    let cfg = parse_args().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    let use_system_idle = system_idle().is_some();
    let mut state = AppState::new();

    enable_raw_mode().unwrap();
    crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture).unwrap();
    let mut term = Terminal::new(CrosstermBackend::new(io::stdout())).unwrap();

    loop {
        match run_tui(&cfg, use_system_idle, &mut state, &mut term) {
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
