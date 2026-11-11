//! The drill-down pages: a host, a service and an event. Like the views they
//! adapt to the width: charts and blocks stack on narrow screens, and tables
//! drop their low-priority columns.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row, Wrap};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

use super::app::{App, Hits, HostPage, ListId, Page, ServicePage, sorted_services};
use super::charts::{self, Charts};
use super::layout::{self, Size, ellipsize};
use super::ui::{
    ServiceView, avail, block, block_focus, describe_mute, dim, event_cols, event_row, header_of,
    longest_subject, render_list, service_cols, service_row, service_status, style, title,
    widths_of,
};
use crate::fmt::{
    self, Tone, event_badge, event_tone, host_badge, host_tone, service_badge, service_tone,
    short_duration,
};
use crate::types::Event;

/// Below this many rows the charts are left out to keep the lists usable.
const CHARTS_MIN_HEIGHT: u16 = 22;

pub fn render(f: &mut Frame, app: &mut App, area: Rect, size: Size) {
    let App {
        pages,
        hits,
        events,
        ..
    } = app;
    match pages.last_mut() {
        Some(Page::Host(page)) => host_page(f, hits, page, area, size),
        Some(Page::Service(page)) => service_page(f, hits, page, area, size),
        Some(Page::Event(page)) => {
            let event = events
                .iter()
                .find(|e| e.id == page.event.id)
                .unwrap_or(&page.event);
            event_page(f, event, area);
        }
        None => {}
    }
}

/// `name  value` with the value cut to the width of the box.
fn field(name: &str, value: impl AsRef<str>, width: u16) -> Line<'static> {
    let room = usize::from(width).saturating_sub(13);
    Line::from(vec![
        Span::styled(format!(" {name:<10}"), dim()),
        Span::raw(ellipsize(value.as_ref(), room)),
    ])
}

fn placeholder(f: &mut Frame, area: Rect, heading: &str, error: Option<&String>) {
    let (text, tone) = match error {
        Some(error) => (error.as_str(), Tone::Bad),
        None => ("loading…", Tone::Dim),
    };
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {text}"), style(tone)))
            .wrap(Wrap { trim: false })
            .block(block(title(heading))),
        area,
    );
}

fn host_page(f: &mut Frame, hits: &mut Hits, page: &mut HostPage, area: Rect, size: Size) {
    let heading = if size.narrow() {
        format!("{} · {}", ellipsize(&page.name, 18), page.range.label())
    } else {
        format!("{} · charts: last {}", page.name, page.range.label())
    };
    let Some(data) = page.data.as_ref() else {
        return placeholder(f, area, &heading, page.error.as_ref());
    };
    let h = &data.summary;
    let width = area.width;
    let os = [&h.os.name, &h.os.release, &h.os.machine]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let mut info = vec![
        Line::from(vec![
            Span::styled(format!(" {:<10}", "state"), dim()),
            Span::styled(host_badge(h.state), style(host_tone(h.state))),
            Span::styled(
                format!(
                    "  {}",
                    ellipsize(&h.hostname, usize::from(width).saturating_sub(26))
                ),
                dim(),
            ),
        ]),
        field("os", if os.is_empty() { "-" } else { os.as_str() }, width),
        field(
            "seen",
            format!(
                "{} ago · up {}",
                short_duration(fmt::now() - h.last_seen),
                h.system
                    .uptime
                    .map_or_else(|| "-".to_owned(), short_duration)
            ),
            width,
        ),
        field("load", fmt::load(h.system.load), width),
        field(
            "monit",
            match (&data.monit_url, h.can_act) {
                (Some(url), true) => url.clone(),
                (Some(url), false) => format!("{url} (cannot act)"),
                (None, _) => "unknown".to_owned(),
            },
            width,
        ),
        field(
            "muted",
            h.muted_until
                .as_ref()
                .map_or_else(|| "no".to_owned(), describe_mute),
            width,
        ),
    ];
    if let Some(error) = &page.error {
        info.push(Line::from(Span::styled(
            format!(
                " {}",
                ellipsize(error, usize::from(width).saturating_sub(4))
            ),
            style(Tone::Bad),
        )));
    }

    let has_system = data.services.iter().any(|s| s.kind == "system");
    let info_height = info.len() as u16 + 2;
    let (info_area, charts_area, list_area) = if area.height >= CHARTS_MIN_HEIGHT {
        let [info_area, charts_area, list_area] = Layout::vertical([
            Constraint::Length(info_height),
            Constraint::Fill(3),
            Constraint::Fill(2),
        ])
        .areas(area);
        (info_area, Some(charts_area), list_area)
    } else {
        let [info_area, list_area] = Layout::vertical([
            Constraint::Length(info_height.min(area.height / 2)),
            Constraint::Min(3),
        ])
        .areas(area);
        (info_area, None, list_area)
    };

    let sorted = sorted_services(data);
    let name_longest = sorted.iter().map(|s| s.name.width()).max().unwrap_or(0);
    let planned = layout::plan(
        &service_cols(size.narrow(), None, name_longest),
        avail(list_area),
    );
    let rows: Vec<Row> = sorted
        .into_iter()
        .map(|s| {
            service_row(
                &planned,
                &ServiceView {
                    state: s.state,
                    stale: false,
                    host: "",
                    name: &s.name,
                    kind: &s.kind,
                    status: service_status(&s.status_text, s.pending_action.as_deref()),
                    since: s.state_since,
                },
            )
        })
        .collect();
    let count = rows.len();

    f.render_widget(
        Paragraph::new(info).block(block(title(&heading))),
        info_area,
    );
    if let Some(charts_area) = charts_area {
        if has_system {
            charts::render(
                f,
                charts_area,
                &page.charts,
                charts::specs_for("system"),
                page.range,
                page.chart_focus,
            );
        } else {
            let none = Charts::Failed("this host reports no system service".to_owned());
            charts::render(
                f,
                charts_area,
                &none,
                charts::specs_for("system"),
                page.range,
                0,
            );
        }
    }
    render_list(
        f,
        hits,
        ListId::HostServices,
        list_area,
        block_focus(title(&format!("Services · {count}")), true),
        Some(header_of(&planned)),
        rows,
        &widths_of(&planned),
        &mut page.cursor,
        true,
    );
}

fn service_page(f: &mut Frame, hits: &mut Hits, page: &mut ServicePage, area: Rect, size: Size) {
    let narrow = size.narrow();
    let subject = format!("{}/{}", page.host, page.service);
    let Some(data) = page.data.as_ref() else {
        return placeholder(f, area, &subject, page.error.as_ref());
    };
    let s = &data.service;
    let width = area.width;
    let mut status = vec![
        Line::from(vec![
            Span::styled(format!(" {:<10}", "state"), dim()),
            Span::styled(service_badge(s.state), style(service_tone(s.state))),
        ]),
        field("type", &s.kind, width),
        field(
            "status",
            service_status(&s.status_text, s.pending_action.as_deref()),
            width,
        ),
        field("since", fmt::ago(s.state_since), width),
        field("monitor", &s.monitor_mode, width),
    ];
    if let Some(every) = &s.every {
        status.push(field("every", every, width));
    }
    if !s.groups.is_empty() {
        status.push(field("groups", s.groups.join(", "), width));
    }
    if let Some(error) = &page.error {
        status.push(Line::from(Span::styled(
            format!(
                " {}",
                ellipsize(error, usize::from(width).saturating_sub(4))
            ),
            style(Tone::Bad),
        )));
    }
    let mut pairs = Vec::new();
    flatten(&data.data, "", 0, &mut pairs);
    let data_width = if narrow { width } else { width * 58 / 100 };
    let key_width = pairs
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0)
        .min(usize::from(data_width / 2));
    let values: Vec<Line> = pairs
        .iter()
        .map(|(k, v)| {
            let room = usize::from(data_width).saturating_sub(key_width + 6);
            Line::from(vec![
                Span::styled(format!(" {:<key_width$}  ", ellipsize(k, key_width)), dim()),
                Span::raw(ellipsize(v, room)),
            ])
        })
        .collect();

    let specs = charts::specs_for(&s.kind);
    let planned = layout::plan(
        &event_cols(narrow, longest_subject(&data.recent_events)),
        avail(area),
    );
    let rows: Vec<Row> = data
        .recent_events
        .iter()
        .map(|e| event_row(&planned, e, narrow))
        .collect();
    let count = rows.len();

    let heading = if narrow {
        format!("{} · {}", ellipsize(&subject, 20), page.range.label())
    } else {
        format!("{subject} · charts: last {}", page.range.label())
    };
    let show_charts = area.height >= CHARTS_MIN_HEIGHT && !specs.is_empty();
    let status_height = status.len() as u16 + 2;
    let data_lines = if values.is_empty() {
        vec![Line::from(Span::styled(" no data reported", dim()))]
    } else {
        values
    };
    let list_area;
    let mut charts_area = None;
    if narrow {
        // one column: status, data (only when there is room), charts, events
        let data_height = if area.height >= 24 {
            (data_lines.len() as u16).clamp(1, 8) + 2
        } else {
            0
        };
        let [status_area, data_area, charts, list] = Layout::vertical([
            Constraint::Length(status_height.min(area.height / 2)),
            Constraint::Length(data_height),
            Constraint::Fill(if show_charts { 3 } else { 0 }),
            Constraint::Fill(2),
        ])
        .areas(area);
        f.render_widget(
            Paragraph::new(status).block(block(title(&heading))),
            status_area,
        );
        if data_height > 0 {
            f.render_widget(
                Paragraph::new(data_lines).block(block(title("Data"))),
                data_area,
            );
        }
        if show_charts {
            charts_area = Some(charts);
        }
        list_area = list;
    } else {
        let top_height = (status.len().max(data_lines.len()).clamp(5, 12) + 2) as u16;
        let (top_area, charts, list) = if show_charts {
            let [top, charts, list] = Layout::vertical([
                Constraint::Length(top_height),
                Constraint::Fill(3),
                Constraint::Fill(2),
            ])
            .areas(area);
            (top, Some(charts), list)
        } else {
            let [top, list] = Layout::vertical([
                Constraint::Length(top_height.min(area.height / 2)),
                Constraint::Min(3),
            ])
            .areas(area);
            (top, None, list)
        };
        let [status_area, data_area] =
            Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
                .areas(top_area);
        f.render_widget(
            Paragraph::new(status).block(block(title(&heading))),
            status_area,
        );
        f.render_widget(
            Paragraph::new(data_lines).block(block(title("Data"))),
            data_area,
        );
        charts_area = charts;
        list_area = list;
    }
    if let Some(charts_area) = charts_area {
        charts::render(
            f,
            charts_area,
            &page.charts,
            specs,
            page.range,
            page.chart_focus,
        );
    }
    render_list(
        f,
        hits,
        ListId::ServiceEvents,
        list_area,
        block_focus(title(&format!("Recent events · {count}")), true),
        Some(header_of(&planned)),
        rows,
        &widths_of(&planned),
        &mut page.cursor,
        true,
    );
    if count == 0 {
        let inner = Rect {
            x: list_area.x + 1,
            y: list_area.y + 2,
            width: list_area.width.saturating_sub(2),
            height: list_area.height.saturating_sub(3),
        };
        f.render_widget(
            Paragraph::new(Span::styled(" no events for this service", dim())),
            inner,
        );
    }
}

fn event_page(f: &mut Frame, e: &Event, area: Rect) {
    let width = area.width;
    let subject = match (&e.host, &e.service) {
        (Some(h), Some(s)) => format!("{h} / {s}"),
        (Some(h), None) => h.clone(),
        _ => "-".to_owned(),
    };
    let acked = if e.acked() {
        format!(
            "by {} {}",
            e.acked_by.as_deref().unwrap_or("?"),
            e.acked_at
                .map_or_else(String::new, |t| format!("at {}", fmt::clock(t)))
        )
    } else {
        "not yet".to_owned()
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<10}", "state"), dim()),
            Span::styled(event_badge(e.state), style(event_tone(e.state))),
            Span::styled(format!("   {}", ellipsize(&e.kind_label, 30)), dim()),
        ]),
        field(
            "when",
            format!(
                "{} ({} ago)",
                fmt::clock(e.created_at),
                short_duration(fmt::now() - e.created_at)
            ),
            width,
        ),
        field("subject", subject, width),
        field("kind", &e.kind, width),
        field("action", e.action.as_deref().unwrap_or("-"), width),
        field("source", &e.source, width),
        field("acked", acked, width),
        Line::from(""),
        Line::from(Span::styled(" Message", dim())),
    ];
    lines.push(Line::from(format!(" {}", e.message)));
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block_focus(title(&format!("Event #{}", e.id)), true)),
        area,
    );
}

/// Flattens JSON into `key  value` pairs, two levels deep and at most 40 of them.
pub fn flatten(value: &Value, prefix: &str, depth: usize, out: &mut Vec<(String, String)>) {
    const MAX: usize = 40;
    if out.len() >= MAX {
        return;
    }
    match value {
        Value::Null if prefix.is_empty() => {}
        Value::Object(map) => {
            for (key, v) in map {
                let name = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match v {
                    Value::Object(_) if depth < 2 => flatten(v, &name, depth + 1, out),
                    Value::Array(items) if items.iter().any(|i| i.is_object()) => {
                        for (i, item) in items.iter().take(6).enumerate() {
                            flatten(item, &format!("{name}[{i}]"), depth + 1, out);
                        }
                        if items.len() > 6 {
                            out.push((name, format!("… {} entries", items.len())));
                        }
                    }
                    _ => out.push((name, scalar(v))),
                }
            }
        }
        other => out.push((prefix.to_owned(), scalar(other))),
    }
    out.truncate(MAX);
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "-".to_owned(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() => format!("{f:.2}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned(),
            _ => n.to_string(),
        },
        Value::Array(items) => items
            .iter()
            .take(8)
            .map(scalar)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(_) => "{…}".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pairs(value: Value) -> Vec<(String, String)> {
        let mut out = Vec::new();
        flatten(&value, "", 0, &mut out);
        out
    }

    #[test]
    fn flatten_nested_objects_and_scalars() {
        let out = pairs(
            json!({"pid": 42, "cpu": {"percent": 1.5, "percent_total": 0.0}, "name": "x", "ok": true, "gone": null}),
        );
        let get = |k: &str| {
            out.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("pid"), Some("42"));
        assert_eq!(get("cpu.percent"), Some("1.5"));
        assert_eq!(get("cpu.percent_total"), Some("0"));
        assert_eq!(get("name"), Some("x"));
        assert_eq!(get("ok"), Some("true"));
        assert_eq!(get("gone"), Some("-"));
    }

    #[test]
    fn flatten_arrays() {
        let out =
            pairs(json!({"load": [0.1, 0.25, 0.3], "ports": [{"hostname": "h", "port": 80}]}));
        assert!(out.contains(&("load".into(), "0.1, 0.25, 0.3".into())));
        assert!(out.contains(&("ports[0].port".into(), "80".into())));
        assert!(pairs(Value::Null).is_empty());
    }

    #[test]
    fn flatten_is_bounded() {
        let big: serde_json::Map<String, Value> =
            (0..100).map(|i| (format!("k{i}"), json!(i))).collect();
        assert_eq!(pairs(Value::Object(big)).len(), 40);
    }
}
