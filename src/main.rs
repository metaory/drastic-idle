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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            phase1_secs: 10,
            phase2_secs: 300,
            phase1_cmd: None,
            phase2_cmd: Some(vec!["systemctl".into(), "poweroff".into()]),
        }
    }
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

/// Smooth gradient by remaining time ratio (1 = plenty, 0 = none): green → yellow → red.
fn phase_color(remaining_ratio: f64) -> Color {
    let t = remaining_ratio.clamp(0.0, 1.0);
    let (r, g, b) = if t <= 0.5 {
        let u = t * 2.0; // 0..1 over first half
        (255u8, (255.0 * u).round() as u8, 0)
    } else {
        let u = (t - 0.5) * 2.0; // 0..1 over second half
        ((255.0 * (1.0 - u)).round() as u8, 255, 0)
    };
    Color::Rgb(r, g, b)
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
    phase2_start: Option<Instant>,
    last_input: Instant,
    last_active_window_id: Option<String>,
}

impl AppState {
    fn new() -> Self {
        Self {
            phase1_done: false,
            phase2_start: None,
            last_input: Instant::now(),
            last_active_window_id: active_window_id(),
        }
    }

    fn reset_phases(&mut self) {
        self.phase1_done = false;
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
        state.phase2_start = Some(now);
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
        .constraints([Constraint::Min(0), Constraint::Ratio(3, 4), Constraint::Length(1)])
        .split(frame.area());
    frame.render_widget(body, chunks[0]);
    let gauge_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(1, 2), Constraint::Length(1), Constraint::Ratio(1, 2)])
        .split(chunks[1]);
    let p1_ratio = (!state.phase1_done && idle < cfg.phase1())
        .then(|| idle.as_secs_f64() / cfg.phase1().as_secs_f64())
        .unwrap_or(0.0);
    let p1_time_left_ratio = 1.0 - p1_ratio;
    let p1_color = phase_color(p1_time_left_ratio);
    let gauge1 = if !state.phase1_done && idle < cfg.phase1() {
        Gauge::default()
            .gauge_style(Style::default().fg(p1_color))
            .ratio(p1_ratio)
            .use_unicode(true)
    } else {
        Gauge::default().ratio(0.0)
    };
    frame.render_widget(gauge1, gauge_chunks[0]);
    frame.render_widget(Paragraph::new(""), gauge_chunks[1]);
    let (p2_ratio, p2_color) = state.phase2_start.map(|start| {
        let elapsed = now.saturating_duration_since(start);
        let rem = cfg.phase2().saturating_sub(elapsed);
        let p2_time_left_ratio = rem.as_secs_f64() / cfg.phase2().as_secs_f64();
        (elapsed.as_secs_f64() / cfg.phase2().as_secs_f64(), phase_color(p2_time_left_ratio))
    }).unwrap_or((0.0, Color::DarkGray));
    let gauge2 = Gauge::default()
        .gauge_style(Style::default().fg(p2_color))
        .ratio(p2_ratio)
        .use_unicode(true);
    frame.render_widget(gauge2, gauge_chunks[2]);
    frame.render_widget(footer, chunks[2]);
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
    let a: Vec<String> = std::env::args().skip(1).take(2).collect();
    let cfg = Config {
        phase1_secs: a.get(0).and_then(|s| s.parse().ok()).unwrap_or(10),
        phase2_secs: a.get(1).and_then(|s| s.parse().ok()).unwrap_or(300),
        ..Config::default()
    };

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
