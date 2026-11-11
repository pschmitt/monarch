//! Output helpers shared by the CLI and the TUI: colors, tables and times.

use std::fmt::Display;
use std::io::IsTerminal;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};
use unicode_width::UnicodeWidthStr;

use crate::types::{EventState, HostState, ServiceState};

/// What a piece of text means; the CLI maps it to ANSI, the TUI to colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Good,
    Warn,
    Bad,
    Dim,
    Bold,
}

impl Tone {
    fn ansi(self) -> Option<&'static str> {
        match self {
            Self::Plain => None,
            Self::Good => Some("32"),
            Self::Warn => Some("33"),
            Self::Bad => Some("31"),
            Self::Dim => Some("2"),
            Self::Bold => Some("1"),
        }
    }
}

pub fn host_tone(state: HostState) -> Tone {
    match state {
        HostState::Ok => Tone::Good,
        HostState::Degraded => Tone::Warn,
        HostState::Offline => Tone::Bad,
        HostState::Unknown => Tone::Dim,
    }
}

pub fn host_badge(state: HostState) -> &'static str {
    match state {
        HostState::Ok => "● online",
        HostState::Degraded => "▲ degraded",
        HostState::Offline => "✘ offline",
        HostState::Unknown => "? unknown",
    }
}

pub fn service_tone(state: ServiceState) -> Tone {
    match state {
        ServiceState::Ok => Tone::Good,
        ServiceState::Failed => Tone::Bad,
        ServiceState::Pending | ServiceState::Init => Tone::Warn,
        ServiceState::Unmonitored | ServiceState::Unknown => Tone::Dim,
    }
}

pub fn service_badge(state: ServiceState) -> &'static str {
    match state {
        ServiceState::Ok => "✔ ok",
        ServiceState::Failed => "✘ failed",
        ServiceState::Pending => "… pending",
        ServiceState::Init => "… init",
        ServiceState::Unmonitored => "○ unmonitored",
        ServiceState::Unknown => "? unknown",
    }
}

pub fn event_tone(state: EventState) -> Tone {
    match state {
        EventState::Failed => Tone::Bad,
        EventState::Succeeded => Tone::Good,
        EventState::Changed => Tone::Warn,
        EventState::ChangedNot | EventState::Init | EventState::Unknown => Tone::Dim,
    }
}

pub fn event_badge(state: EventState) -> &'static str {
    match state {
        EventState::Failed => "✘ failed",
        EventState::Succeeded => "✔ succeeded",
        EventState::Changed => "~ changed",
        EventState::ChangedNot => "= unchanged",
        EventState::Init => "… init",
        EventState::Unknown => "? unknown",
    }
}

/// Whether to emit ANSI colors.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    on: bool,
}

impl Palette {
    /// Colors when stdout is a terminal, `NO_COLOR` is unset (or empty) and `TERM` is not `dumb`.
    pub fn detect() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
        Self {
            on: std::io::stdout().is_terminal() && !no_color && !dumb,
        }
    }

    #[cfg(test)]
    pub const fn new(on: bool) -> Self {
        Self { on }
    }

    pub fn paint(self, tone: Tone, text: impl Display) -> String {
        match tone.ansi() {
            Some(code) if self.on => format!("\x1b[{code}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
}

impl Cell {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Tone::Plain)
    }

    pub fn dim(text: impl Into<String>) -> Self {
        Self::new(text, Tone::Dim)
    }
}

/// A borderless, column-aligned table.
#[derive(Debug, Default)]
pub struct Table {
    headers: Vec<&'static str>,
    rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn new(headers: &[&'static str]) -> Self {
        Self {
            headers: headers.to_vec(),
            rows: Vec::new(),
        }
    }

    pub fn row(&mut self, cells: Vec<Cell>) {
        self.rows.push(cells);
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The table with its header line (when it has headers).
    pub fn render(&self, palette: Palette) -> String {
        self.lines(palette, true).join("\n")
    }

    /// Only the rows, without the header line.
    pub fn render_rows(&self, palette: Palette) -> String {
        self.lines(palette, false).join("\n")
    }

    fn lines(&self, palette: Palette, with_header: bool) -> Vec<String> {
        let columns = self
            .rows
            .iter()
            .map(Vec::len)
            .chain([self.headers.len()])
            .max()
            .unwrap_or(0);
        let mut widths = vec![0; columns];
        for (width, header) in widths.iter_mut().zip(&self.headers) {
            *width = header.width();
        }
        for row in &self.rows {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.text.width());
            }
        }
        let line = |cells: Vec<(&str, Tone)>| {
            let mut out = String::new();
            for (i, (text, tone)) in cells.into_iter().enumerate() {
                if i > 0 {
                    out.push_str("  ");
                }
                out.push_str(&palette.paint(tone, text));
                out.push_str(&" ".repeat(widths[i].saturating_sub(text.width())));
            }
            // No trailing padding: it only adds noise when copying or piping.
            let trimmed = out.trim_end_matches(' ').len();
            out.truncate(trimmed);
            out
        };
        let mut lines = Vec::new();
        if with_header && !self.headers.is_empty() {
            lines.push(line(self.headers.iter().map(|h| (*h, Tone::Dim)).collect()));
        }
        lines.extend(
            self.rows
                .iter()
                .map(|row| line(row.iter().map(|c| (c.text.as_str(), c.tone)).collect())),
        );
        lines
    }
}

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `45s`, `7m`, `3h`, `12d`: the largest whole unit.
pub fn short_duration(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

/// `3m ago`, or `-` without a timestamp.
pub fn ago(since: Option<f64>) -> String {
    match since {
        Some(ts) if ts > 0.0 => format!("{} ago", short_duration(now() - ts)),
        _ => "-".to_owned(),
    }
}

/// Local time of day, with the date when it is not today.
pub fn clock(ts: f64) -> String {
    let Some(time) = Local.timestamp_opt(ts as i64, 0).single() else {
        return "-".to_owned();
    };
    if time.date_naive() == Local::now().date_naive() {
        time.format("%H:%M:%S").to_string()
    } else {
        time.format("%m-%d %H:%M:%S").to_string()
    }
}

pub fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| format!("{v:.0}%"))
}

pub fn load(load: Option<[f64; 3]>) -> String {
    load.map_or_else(
        || "-".to_owned(),
        |[a, b, c]| format!("{a:.2} {b:.2} {c:.2}"),
    )
}

/// A one-line chart of `values` using block characters, `width` cells wide.
/// Values up to 100 are scaled against 100 (percentages), larger ones against their maximum.
pub fn sparkline(values: &[f64], width: usize) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if width == 0 || values.is_empty() {
        return " ".repeat(width);
    }
    let chunk = values.len().div_ceil(width);
    let points: Vec<f64> = values
        .chunks(chunk)
        .map(|c| c.iter().sum::<f64>() / c.len() as f64)
        .collect();
    let max = points.iter().copied().fold(0.0_f64, f64::max);
    let scale = if max <= 100.0 { 100.0 } else { max };
    let bars: String = points
        .iter()
        .map(|v| BARS[((v / scale) * 7.0).round().clamp(0.0, 7.0) as usize])
        .collect();
    format!("{}{bars}", " ".repeat(width.saturating_sub(points.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_off_is_plain() {
        assert_eq!(Palette::new(false).paint(Tone::Bad, "x"), "x");
        assert_eq!(Palette::new(true).paint(Tone::Bad, "x"), "\x1b[31mx\x1b[0m");
        assert_eq!(Palette::new(true).paint(Tone::Plain, "x"), "x");
    }

    #[test]
    fn table_aligns_columns_without_trailing_padding() {
        let mut table = Table::new(&["NAME", "STATE"]);
        table.row(vec![Cell::plain("a"), Cell::plain("ok")]);
        table.row(vec![Cell::plain("longer"), Cell::plain("failed")]);
        let out = table.render(Palette::new(false));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines, ["NAME    STATE", "a       ok", "longer  failed"]);
    }

    #[test]
    fn durations() {
        assert_eq!(short_duration(5.0), "5s");
        assert_eq!(short_duration(125.0), "2m");
        assert_eq!(short_duration(7300.0), "2h");
        assert_eq!(short_duration(200_000.0), "2d");
        assert_eq!(short_duration(-3.0), "0s");
        assert_eq!(ago(None), "-");
    }

    #[test]
    fn sparkline_scales_and_pads() {
        assert_eq!(sparkline(&[], 4), "    ");
        assert_eq!(sparkline(&[0.0, 100.0], 4), "  ▁█");
        assert_eq!(sparkline(&[50.0; 8], 4).chars().count(), 4);
        // large values scale against their own maximum
        assert_eq!(sparkline(&[0.0, 1000.0], 2), "▁█");
    }
}
