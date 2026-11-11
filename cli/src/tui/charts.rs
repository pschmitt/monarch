//! Metric history: which metrics a service type has, parsing of
//! `GET /api/metrics` and drawing multi-row line charts with axes and stats.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, BorderType, Chart, Dataset, GraphType, Paragraph};

use crate::types::MetricsResponse;

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::Rgb(128, 132, 148);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Percent,
    Number,
    Millis,
    Kilobytes,
    Bytes,
    BytesPerSec,
    Count,
}

/// One metric of a service type.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: Unit,
    pub color: Color,
}

const fn spec(key: &'static str, label: &'static str, unit: Unit, color: Color) -> Spec {
    Spec {
        key,
        label,
        unit,
        color,
    }
}

/// The metrics worth charting for a service type (keys from `docs/API.md`).
pub fn specs_for(kind: &str) -> &'static [Spec] {
    const SYSTEM: [Spec; 3] = [
        spec("cpu", "CPU", Unit::Percent, ACCENT),
        spec("mem_percent", "Memory", Unit::Percent, Color::Magenta),
        spec("load1", "Load (1m)", Unit::Number, Color::Yellow),
    ];
    const PROCESS: [Spec; 3] = [
        spec("cpu", "CPU", Unit::Percent, ACCENT),
        spec("mem_kb", "Memory", Unit::Kilobytes, Color::Magenta),
        spec("threads", "Threads", Unit::Count, Color::Yellow),
    ];
    const FILESYSTEM: [Spec; 3] = [
        spec("space_percent", "Space used", Unit::Percent, ACCENT),
        spec("read_bps", "Read", Unit::BytesPerSec, Color::Green),
        spec("write_bps", "Write", Unit::BytesPerSec, Color::Magenta),
    ];
    const NET: [Spec; 2] = [
        spec("rx_bps", "Download", Unit::BytesPerSec, Color::Green),
        spec("tx_bps", "Upload", Unit::BytesPerSec, Color::Magenta),
    ];
    const HOST: [Spec; 2] = [
        spec("icmp_ms", "Ping", Unit::Millis, ACCENT),
        spec("port_ms", "Port response", Unit::Millis, Color::Magenta),
    ];
    const FILE: [Spec; 1] = [spec("size", "Size", Unit::Bytes, ACCENT)];
    const PROGRAM: [Spec; 1] = [spec("exit_status", "Exit status", Unit::Count, ACCENT)];
    match kind {
        "system" => &SYSTEM,
        "process" => &PROCESS,
        "filesystem" => &FILESYSTEM,
        "net" => &NET,
        "host" => &HOST,
        "file" => &FILE,
        "program" => &PROGRAM,
        _ => &[],
    }
}

/// How far back the charts reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    Hour,
    #[default]
    SixHours,
    Day,
    Week,
}

impl Range {
    pub fn secs(self) -> f64 {
        match self {
            Self::Hour => 3600.0,
            Self::SixHours => 6.0 * 3600.0,
            Self::Day => 86_400.0,
            Self::Week => 7.0 * 86_400.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hour => "1h",
            Self::SixHours => "6h",
            Self::Day => "24h",
            Self::Week => "7d",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Hour => Self::SixHours,
            Self::SixHours => Self::Day,
            Self::Day => Self::Week,
            Self::Week => Self::Hour,
        }
    }
}

/// One metric's history: `(seconds relative to now (<= 0), value)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub key: String,
    pub points: Vec<(f64, f64)>,
}

/// The state of the charts of a page.
#[derive(Debug, Clone, Default)]
pub enum Charts {
    #[default]
    Loading,
    Failed(String),
    Ready(Vec<Series>),
}

pub fn parse(response: &MetricsResponse, now: f64) -> Vec<Series> {
    response
        .series
        .iter()
        .map(|series| {
            let mut points: Vec<(f64, f64)> = series
                .points
                .iter()
                .filter_map(|p| {
                    Some((
                        p.first().copied().flatten()? - now,
                        p.get(1).copied().flatten()?,
                    ))
                })
                .collect();
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
            Series {
                key: series.metric.clone(),
                points,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub last: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
}

pub fn stats(points: &[(f64, f64)]) -> Option<Stats> {
    let last = points.last()?.1;
    let values = points.iter().map(|p| p.1);
    Some(Stats {
        last,
        min: values.clone().fold(f64::INFINITY, f64::min),
        max: values.clone().fold(f64::NEG_INFINITY, f64::max),
        avg: values.sum::<f64>() / points.len() as f64,
    })
}

/// The smallest "round" number (1, 2, 2.5, 5 times a power of ten) that is at least `value`.
pub fn nice_ceil(value: f64) -> f64 {
    if value <= 0.0 || !value.is_finite() {
        return 1.0;
    }
    let magnitude = 10f64.powf(value.log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|candidate| *candidate >= value - f64::EPSILON * value)
        .unwrap_or(10.0 * magnitude)
}

fn human_bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes;
    let mut unit = 0;
    while value.abs() >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value.abs() >= 10.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

pub fn format_value(unit: Unit, v: f64) -> String {
    match unit {
        Unit::Percent if v < 10.0 => format!("{v:.1}%"),
        Unit::Percent => format!("{v:.0}%"),
        Unit::Number if v < 10.0 => format!("{v:.2}"),
        Unit::Number => format!("{v:.1}"),
        Unit::Millis if v < 10.0 => format!("{v:.1}ms"),
        Unit::Millis => format!("{v:.0}ms"),
        Unit::Kilobytes => human_bytes(v * 1024.0),
        Unit::Bytes => human_bytes(v),
        Unit::BytesPerSec => format!("{}/s", human_bytes(v)),
        Unit::Count => format!("{v:.0}"),
    }
}

fn dim() -> Style {
    Style::new().fg(DIM)
}

fn block(title: Line<'static>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(dim())
        .title(title)
}

fn message(f: &mut Frame, area: Rect, title: &str, text: &str) {
    let title = Line::from(Span::styled(
        format!(" {title} "),
        Style::new().fg(ACCENT).bold(),
    ));
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {text}"), dim())).block(block(title)),
        area,
    );
}

/// Draws one chart per metric of `specs` side by side in `area`.
/// Wide areas put the charts side by side. Narrow ones stack them, or, when
/// they would get too few rows each, show one at a time (`focus` picks it).
pub fn render(
    f: &mut Frame,
    area: Rect,
    charts: &Charts,
    specs: &[Spec],
    range: Range,
    focus: usize,
) {
    let heading = format!("Metrics · last {}", range.label());
    let series = match charts {
        Charts::Loading => return message(f, area, &heading, "loading…"),
        Charts::Failed(error) => return message(f, area, &heading, error),
        Charts::Ready(series) => series,
    };
    let shown: Vec<(&Spec, &Series)> = specs
        .iter()
        .filter_map(|spec| {
            series
                .iter()
                .find(|s| s.key == spec.key && !s.points.is_empty())
                .map(|s| (spec, s))
        })
        .collect();
    if shown.is_empty() {
        return message(f, area, &heading, "no history for this period yet");
    }
    let count = shown.len();
    if area.width >= 80 {
        let cells = Layout::horizontal(vec![Constraint::Ratio(1, count as u32); count]).split(area);
        for ((spec, series), cell) in shown.into_iter().zip(cells.iter()) {
            render_one(f, *cell, spec, series, range, None);
        }
    } else if usize::from(area.height) / count >= MIN_STACKED_HEIGHT {
        let cells = Layout::vertical(vec![Constraint::Ratio(1, count as u32); count]).split(area);
        for ((spec, series), cell) in shown.into_iter().zip(cells.iter()) {
            render_one(f, *cell, spec, series, range, None);
        }
    } else {
        let index = focus % count;
        let (spec, series) = shown[index];
        render_one(
            f,
            area,
            spec,
            series,
            range,
            (count > 1).then_some((index + 1, count)),
        );
    }
}

/// Rows a chart needs to be readable when stacked.
const MIN_STACKED_HEIGHT: usize = 7;

fn render_one(
    f: &mut Frame,
    area: Rect,
    spec: &Spec,
    series: &Series,
    range: Range,
    position: Option<(usize, usize)>,
) {
    let Some(stat) = stats(&series.points) else {
        return;
    };
    let y_max = match spec.unit {
        Unit::Percent => nice_ceil((stat.max * 1.15).max(5.0)).min(100.0),
        _ => nice_ceil(stat.max * 1.15),
    };
    let secs = range.secs();
    let mut title = vec![
        Span::styled(
            format!(" {} ", spec.label),
            Style::new().fg(spec.color).bold(),
        ),
        Span::styled("now ", dim()),
        Span::styled(format_value(spec.unit, stat.last), Style::new().bold()),
    ];
    let wide = usize::from(area.width);
    if wide >= 40 {
        title.push(Span::styled(
            format!("  max {}", format_value(spec.unit, stat.max)),
            dim(),
        ));
    }
    if wide >= 58 {
        title.push(Span::styled(
            format!(
                "  avg {}  min {}",
                format_value(spec.unit, stat.avg),
                format_value(spec.unit, stat.min)
            ),
            dim(),
        ));
    }
    if let Some((index, count)) = position {
        title.push(Span::styled(format!("  {index}/{count} c: next"), dim()));
    }
    title.push(Span::raw(" "));

    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::new().fg(spec.color))
        .data(&series.points);
    let x_labels = vec![
        Span::styled(format!("-{}", range.label()), dim()),
        Span::styled(format!("-{}", half_label(range)), dim()),
        Span::styled("now", dim()),
    ];
    let y_labels = vec![
        Span::styled("0", dim()),
        Span::styled(format_value(spec.unit, y_max / 2.0), dim()),
        Span::styled(format_value(spec.unit, y_max), dim()),
    ];
    let chart = Chart::new(vec![dataset])
        .block(block(Line::from(title)))
        .x_axis(
            Axis::default()
                .bounds([-secs, 0.0])
                .labels(x_labels)
                .style(dim()),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, y_max])
                .labels(y_labels)
                .style(dim()),
        );
    f.render_widget(chart, area);
}

fn half_label(range: Range) -> &'static str {
    match range {
        Range::Hour => "30m",
        Range::SixHours => "3h",
        Range::Day => "12h",
        Range::Week => "3d12h",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    #[test]
    fn nice_ceil_rounds_up_to_round_numbers() {
        assert_eq!(nice_ceil(0.0), 1.0);
        assert_eq!(nice_ceil(0.7), 1.0);
        assert_eq!(nice_ceil(1.2), 2.0);
        assert_eq!(nice_ceil(2.2), 2.5);
        assert_eq!(nice_ceil(3.0), 5.0);
        assert_eq!(nice_ceil(5.0), 5.0);
        assert_eq!(nice_ceil(61.0), 100.0);
        assert_eq!(nice_ceil(0.03), 0.05);
    }

    #[test]
    fn values_are_formatted_by_unit() {
        assert_eq!(format_value(Unit::Percent, 3.17), "3.2%");
        assert_eq!(format_value(Unit::Percent, 42.0), "42%");
        assert_eq!(format_value(Unit::Number, 0.456), "0.46");
        assert_eq!(format_value(Unit::Millis, 12.4), "12ms");
        assert_eq!(format_value(Unit::Kilobytes, 2048.0), "2.0M");
        assert_eq!(format_value(Unit::BytesPerSec, 1536.0), "1.5K/s");
        assert_eq!(format_value(Unit::Count, 7.0), "7");
    }

    #[test]
    fn parse_skips_null_points_and_is_relative_to_now() {
        let response: MetricsResponse = serde_json::from_value(serde_json::json!({
            "resolution": 300,
            "series": [{"metric": "cpu", "points": [[1000, 2.0, 1.0, 3.0], [700, 1.0, null, null], [1300, null, null, null]]}]
        }))
        .unwrap();
        let series = parse(&response, 1000.0);
        assert_eq!(series[0].points, vec![(-300.0, 1.0), (0.0, 2.0)]);
        let s = stats(&series[0].points).unwrap();
        assert_eq!((s.last, s.min, s.max, s.avg), (2.0, 1.0, 2.0, 1.5));
        assert!(stats(&[]).is_none());
    }

    #[test]
    fn specs_exist_for_chartable_types() {
        assert_eq!(specs_for("system").len(), 3);
        assert!(specs_for("directory").is_empty());
        assert_eq!(specs_for("process")[0].key, "cpu");
    }

    fn draw(charts: &Charts, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|f| render(f, f.area(), charts, specs_for("system"), Range::SixHours, 0))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn charts_show_axes_and_stats() {
        let points: Vec<(f64, f64)> = (0..72)
            .map(|i| (-21_600.0 + f64::from(i) * 300.0, f64::from(i % 20)))
            .collect();
        let series = ["cpu", "mem_percent", "load1"]
            .iter()
            .map(|k| Series {
                key: (*k).to_owned(),
                points: points.clone(),
            })
            .collect();
        let screen = draw(&Charts::Ready(series), 120, 12);
        for needle in ["CPU", "Memory", "Load (1m)", "now", "max", "-6h", "-3h"] {
            assert!(screen.contains(needle), "missing {needle}:\n{screen}");
        }
    }

    #[test]
    fn placeholders() {
        assert!(draw(&Charts::Loading, 60, 5).contains("loading"));
        assert!(draw(&Charts::Failed("boom".into()), 60, 5).contains("boom"));
        assert!(draw(&Charts::Ready(vec![]), 60, 5).contains("no history"));
    }
}
