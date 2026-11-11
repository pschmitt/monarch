//! Rendering. Reads [`App`] and draws; the only state it touches is the
//! table scroll offsets, which ratatui updates while rendering.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, HighlightSpacing, Paragraph, Row, Sparkline, Table,
    Tabs,
};

use super::app::{App, Conn, Detail, Mode, Pending, Tab};
use crate::fmt::{
    self, Tone, event_badge, event_tone, host_badge, host_tone, service_badge, service_tone,
    short_duration,
};
use crate::types::{Event, HostState, HostSummary, MutedUntil, Service, ServiceListItem};

const ACCENT: Color = Color::Cyan;
const SELECT_BG: Color = Color::Rgb(38, 44, 66);
const DIM: Color = Color::Rgb(128, 132, 148);
const MEM: Color = Color::Magenta;

fn style(tone: Tone) -> Style {
    match tone {
        Tone::Plain => Style::new(),
        Tone::Good => Style::new().fg(Color::Green),
        Tone::Warn => Style::new().fg(Color::Yellow),
        Tone::Bad => Style::new().fg(Color::Red),
        Tone::Dim => Style::new().fg(DIM),
        Tone::Bold => Style::new().add_modifier(Modifier::BOLD),
    }
}

fn dim() -> Style {
    style(Tone::Dim)
}

fn block(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(dim())
        .title(title.into())
}

fn title(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {text} "),
        Style::new().fg(ACCENT).bold(),
    ))
}

pub fn render(f: &mut Frame, app: &mut App) {
    let [header, tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    render_header(f, app, header);
    render_tabs(f, app, tabs);
    if app.detail.is_some() {
        render_detail(f, app, body);
    } else {
        match app.tab {
            Tab::Overview => render_overview(f, app, body),
            Tab::Hosts => render_hosts(f, app, body),
            Tab::Services => render_services(f, app, body),
            Tab::Events => render_events(f, app, body),
        }
    }
    render_footer(f, app, footer);

    match &app.mode {
        Mode::Confirm(pending) => render_confirm(f, pending, body),
        Mode::Help => render_help(f, body),
        Mode::Normal | Mode::Search => {}
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let left = Line::from(vec![
        Span::styled(" ◆ monarch", Style::new().fg(ACCENT).bold()),
        Span::styled(format!("  {}", app.base_url), dim()),
    ]);
    f.render_widget(Paragraph::new(left), area);

    let count =
        |hosts: &[HostSummary], state: HostState| hosts.iter().filter(|h| h.state == state).count();
    let (offline, degraded) = (
        count(&app.hosts, HostState::Offline),
        count(&app.hosts, HostState::Degraded),
    );
    let online = app.hosts.len() - offline - degraded;
    let (conn_text, conn_tone) = match (&app.api_error, app.conn) {
        (Some(_), _) => ("✘ offline", Tone::Bad),
        (None, Conn::Live) => ("● live", Tone::Good),
        (None, Conn::Connecting) => ("○ connecting", Tone::Warn),
        (None, Conn::Reconnecting) => ("○ reconnecting", Tone::Warn),
    };
    let mut spans = vec![
        Span::styled(format!("● {online} "), style(Tone::Good)),
        Span::styled(format!("▲ {degraded} "), style(Tone::Warn)),
        Span::styled(format!("✘ {offline}"), style(Tone::Bad)),
        Span::styled("  │  ", dim()),
    ];
    if app.read_only() {
        spans.push(Span::styled("read-only  ", style(Tone::Warn)));
    }
    spans.push(Span::styled(format!("{conn_text} "), style(conn_tone)));
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Right),
        area,
    );
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let failed = app.overview.services.failed;
    let unacked = app.unacked_count();
    let titles = vec![
        " Overview ".to_owned(),
        format!(" Hosts ({}) ", app.hosts.len()),
        if failed > 0 {
            format!(" Services ({failed} failed) ")
        } else {
            format!(" Services ({}) ", app.services.len())
        },
        if unacked > 0 {
            format!(" Events ({unacked} new) ")
        } else {
            " Events ".to_owned()
        },
    ];
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .style(dim())
        .highlight_style(
            Style::new()
                .fg(ACCENT)
                .bold()
                .add_modifier(Modifier::UNDERLINED),
        )
        .divider(Span::styled("│", dim()));
    f.render_widget(tabs, area);
}

fn hint(key: &'static str, what: &'static str) -> [Span<'static>; 2] {
    [
        Span::styled(key, Style::new().fg(ACCENT)),
        Span::styled(format!(" {what}   "), dim()),
    ]
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    if app.mode == Mode::Search {
        let line = Line::from(vec![
            Span::styled(" / ", Style::new().fg(ACCENT).bold()),
            Span::raw(app.search_text().to_owned()),
            Span::styled("▏", Style::new().fg(ACCENT)),
            Span::styled("    Enter apply   Esc clear", dim()),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    if let Some(toast) = app.toast.as_ref().filter(|t| t.visible()) {
        let line = Line::from(Span::styled(
            format!(" {}", toast.text),
            style(toast.tone).bold(),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let mut spans = vec![Span::raw(" ")];
    let hints: &[(&'static str, &'static str)] = if app.detail.is_some() {
        &[
            ("j/k", "move"),
            ("r", "restart"),
            ("s", "start"),
            ("S", "stop"),
            ("m", "monitor"),
            ("R", "remonitor"),
            ("Esc", "back"),
            ("?", "help"),
        ]
    } else {
        match app.tab {
            Tab::Overview => &[
                ("Tab", "next view"),
                ("^r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Tab::Hosts => &[
                ("j/k", "move"),
                ("Enter", "details"),
                ("/", "search"),
                ("f", "state"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Tab::Services => &[
                ("j/k", "move"),
                ("Enter", "host"),
                ("/", "search"),
                ("f", "state"),
                ("r", "restart"),
                ("m", "monitor"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Tab::Events => &[
                ("j/k", "move"),
                ("a", "ack"),
                ("A", "ack all"),
                ("u", "unacked"),
                ("/", "search"),
                ("Enter", "host"),
                ("?", "help"),
                ("q", "quit"),
            ],
        }
    };
    for (key, what) in hints {
        spans.extend(hint(key, what));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn state_line(symbol: &str, label: &str, value: u64, tone: Tone) -> Line<'static> {
    let tone = if value == 0 { Tone::Dim } else { tone };
    Line::from(vec![
        Span::styled(format!(" {symbol} {label:<12}"), style(tone)),
        Span::styled(format!("{value:>5}"), style(tone).bold()),
    ])
}

fn render_overview(f: &mut Frame, app: &App, area: Rect) {
    let [top, bottom] = Layout::vertical([Constraint::Length(7), Constraint::Min(0)]).areas(area);
    let [hosts_area, services_area, activity_area] =
        Layout::horizontal([Constraint::Ratio(1, 3); 3]).areas(top);
    let o = &app.overview;

    let total = |label: &str, value: u64| {
        Line::from(vec![
            Span::styled(format!("   {label:<12}"), dim()),
            Span::styled(format!("{value:>5}"), Style::new().bold()),
        ])
    };
    let hosts = vec![
        total("total", o.hosts.total),
        state_line(
            "●",
            "online",
            o.hosts
                .total
                .saturating_sub(o.hosts.degraded + o.hosts.offline),
            Tone::Good,
        ),
        state_line("▲", "degraded", o.hosts.degraded, Tone::Warn),
        state_line("✘", "offline", o.hosts.offline, Tone::Bad),
    ];
    f.render_widget(
        Paragraph::new(hosts).block(block(title("Hosts"))),
        hosts_area,
    );

    let services = vec![
        total("total", o.services.total),
        state_line("✔", "ok", o.services.ok, Tone::Good),
        state_line("✘", "failed", o.services.failed, Tone::Bad),
        state_line("…", "pending", o.services.pending, Tone::Warn),
        state_line("○", "unmonitored", o.services.unmonitored, Tone::Dim),
    ];
    f.render_widget(
        Paragraph::new(services).block(block(title("Services"))),
        services_area,
    );

    let activity_block = block(title("Events, 24h"));
    let inner = activity_block.inner(activity_area);
    f.render_widget(activity_block, activity_area);
    let [chart, caption] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let data: Vec<u64> = o.activity.iter().map(|b| b.ok + b.failed).collect();
    f.render_widget(
        Sparkline::default()
            .data(data)
            .style(Style::new().fg(ACCENT)),
        chart,
    );
    let failed: u64 = o.activity.iter().map(|b| b.failed).sum();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{} events", o.events_24h), Style::new().bold()),
            Span::styled(
                format!("  {failed} failed"),
                style(if failed > 0 { Tone::Bad } else { Tone::Dim }),
            ),
        ])),
        caption,
    );

    let [attention_area, events_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(bottom);
    render_attention(f, app, attention_area);

    let capacity = usize::from(events_area.height.saturating_sub(2));
    let lines: Vec<Line> = app.events.iter().take(capacity).map(event_line).collect();
    let lines = if lines.is_empty() {
        vec![Line::from(Span::styled(" no events yet", dim()))]
    } else {
        lines
    };
    f.render_widget(
        Paragraph::new(lines).block(block(title("Recent events"))),
        events_area,
    );
}

fn event_line(e: &Event) -> Line<'static> {
    let subject = match (&e.host, &e.service) {
        (Some(h), Some(s)) => format!("{h}/{s}"),
        (Some(h), None) => h.clone(),
        _ => String::new(),
    };
    Line::from(vec![
        Span::styled(
            format!(" {:>4} ", short_duration(fmt::now() - e.created_at)),
            dim(),
        ),
        Span::styled(
            format!("{} ", event_badge(e.state).chars().next().unwrap_or('?')),
            style(event_tone(e.state)),
        ),
        Span::styled(format!("{subject} "), Style::new().bold()),
        Span::styled(e.message.clone(), dim()),
    ])
}

fn render_attention(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    for h in app.hosts.iter().filter(|h| h.state == HostState::Offline) {
        lines.push(Line::from(vec![
            Span::styled(" ✘ offline   ", style(Tone::Bad)),
            Span::styled(format!("{} ", h.label()), Style::new().bold()),
            Span::styled(
                format!("last seen {} ago", short_duration(fmt::now() - h.last_seen)),
                dim(),
            ),
        ]));
    }
    for h in app.hosts.iter().filter(|h| h.state == HostState::Degraded) {
        lines.push(Line::from(vec![
            Span::styled(" ▲ degraded  ", style(Tone::Warn)),
            Span::styled(format!("{} ", h.label()), Style::new().bold()),
            Span::styled(format!("failing: {}", h.failing.join(", ")), dim()),
        ]));
    }
    for s in &app.overview.failing {
        lines.push(Line::from(vec![
            Span::styled(" ✘ failed    ", style(Tone::Bad)),
            Span::styled(format!("{}/{} ", s.host, s.service), Style::new().bold()),
            Span::styled(format!("{}  {}", s.status_text, fmt::ago(s.since)), dim()),
        ]));
    }
    for s in &app.overview.unmonitored {
        lines.push(Line::from(vec![
            Span::styled(" ○ unmonitored ", dim()),
            Span::styled(format!("{}/{}", s.host, s.service), dim()),
        ]));
    }
    let title_text = if lines.is_empty() {
        "Needs attention".to_owned()
    } else {
        format!("Needs attention ({})", lines.len())
    };
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            if app.loaded {
                " ✔ All systems operational"
            } else {
                " loading…"
            },
            style(if app.loaded { Tone::Good } else { Tone::Dim }),
        )));
    }
    f.render_widget(Paragraph::new(lines).block(block(title(&title_text))), area);
}

fn header_row(columns: &[&'static str]) -> Row<'static> {
    Row::new(columns.iter().map(|c| Cell::from(*c))).style(dim().bold())
}

fn table_block(name: &str, shown: usize, total: usize, extra: &[String]) -> Block<'static> {
    let mut text = format!("{name} · {shown}");
    if shown != total {
        text.push_str(&format!("/{total}"));
    }
    for part in extra {
        text.push_str(&format!(" · {part}"));
    }
    block(title(&text))
}

fn selected_style() -> Style {
    Style::new().bg(SELECT_BG).add_modifier(Modifier::BOLD)
}

fn extras(app: &App, filter: Option<&str>) -> Vec<String> {
    let mut extra = Vec::new();
    if let Some(label) = filter.filter(|l| *l != "all") {
        extra.push(format!("state: {label}"));
    }
    if !app.search_text().is_empty() {
        extra.push(format!("/{}", app.search_text()));
    }
    extra
}

fn empty_hint(f: &mut Frame, area: Rect, loaded: bool, what: &str) {
    let text = if loaded {
        format!("no {what}")
    } else {
        "loading…".to_owned()
    };
    let inner = block("").inner(area);
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {text}"), dim())),
        inner,
    );
}

fn render_hosts(f: &mut Frame, app: &mut App, area: Rect) {
    let extra = extras(app, Some(app.host_filter.label()));
    let rows: Vec<Row> = app.visible_hosts().into_iter().map(host_row).collect();
    let (shown, total) = (rows.len(), app.hosts.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Min(16),
            Constraint::Length(17),
            Constraint::Length(17),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(8),
        ],
    )
    .header(header_row(&[
        "STATE", "HOST", "CPU", "MEM", "LOAD", "SERVICES", "SEEN",
    ]))
    .block(table_block("Hosts", shown, total, &extra))
    .row_highlight_style(selected_style())
    .highlight_symbol("▌")
    .highlight_spacing(HighlightSpacing::Always);
    app.hosts_cursor.sync(shown);
    f.render_stateful_widget(table, area, &mut app.hosts_cursor.state);
    if shown == 0 {
        empty_hint(
            f,
            Rect {
                y: area.y + 1,
                height: area.height.saturating_sub(1),
                ..area
            },
            app.loaded,
            "hosts",
        );
    }
}

fn spark_cell(values: &[f64], percent: Option<f64>, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(fmt::sparkline(values, 10), Style::new().fg(color)),
        Span::raw(format!(" {:>4}", fmt::percent(percent))),
    ])
}

fn host_row(h: &HostSummary) -> Row<'static> {
    let services = if h.services.failed > 0 {
        Line::from(vec![
            Span::raw(format!("{}/{} ", h.services.ok, h.services.total)),
            Span::styled(format!("✘{}", h.services.failed), style(Tone::Bad)),
        ])
    } else {
        Line::from(format!("{}/{}", h.services.ok, h.services.total))
    };
    let mut name = vec![Span::raw(h.label().to_owned())];
    if h.muted_until.is_some() {
        name.push(Span::styled(" (muted)", dim()));
    }
    Row::new([
        Cell::from(Span::styled(host_badge(h.state), style(host_tone(h.state)))),
        Cell::from(Line::from(name)),
        Cell::from(spark_cell(&h.sparkline.cpu, h.system.cpu, ACCENT)),
        Cell::from(spark_cell(&h.sparkline.mem, h.system.mem_percent, MEM)),
        Cell::from(fmt::load(h.system.load)),
        Cell::from(services),
        Cell::from(Span::styled(
            short_duration(fmt::now() - h.last_seen),
            dim(),
        )),
    ])
}

fn badge_cell(state: crate::types::ServiceState, stale: bool) -> Cell<'static> {
    if stale {
        Cell::from(Span::styled("… stale", dim()))
    } else {
        Cell::from(Span::styled(
            service_badge(state),
            style(service_tone(state)),
        ))
    }
}

fn render_services(f: &mut Frame, app: &mut App, area: Rect) {
    let extra = extras(app, Some(app.service_filter.label()));
    let rows: Vec<Row> = app
        .visible_services()
        .into_iter()
        .map(|s: &ServiceListItem| {
            let status = match &s.pending_action {
                Some(action) => format!("{} (pending {action})", s.status_text),
                None => s.status_text.clone(),
            };
            Row::new([
                badge_cell(s.state, s.stale),
                Cell::from(s.host.clone()),
                Cell::from(Span::styled(s.name.clone(), Style::new().bold())),
                Cell::from(Span::styled(s.kind.clone(), dim())),
                Cell::from(status),
                Cell::from(Span::styled(fmt::ago(s.state_since), dim())),
            ])
        })
        .collect();
    let (shown, total) = (rows.len(), app.services.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(15),
            Constraint::Length(20),
            Constraint::Length(22),
            Constraint::Length(11),
            Constraint::Min(16),
            Constraint::Length(12),
        ],
    )
    .header(header_row(&[
        "STATE", "HOST", "SERVICE", "TYPE", "STATUS", "SINCE",
    ]))
    .block(table_block("Services", shown, total, &extra))
    .row_highlight_style(selected_style())
    .highlight_symbol("▌")
    .highlight_spacing(HighlightSpacing::Always);
    app.services_cursor.sync(shown);
    f.render_stateful_widget(table, area, &mut app.services_cursor.state);
    if shown == 0 {
        empty_hint(
            f,
            Rect {
                y: area.y + 1,
                height: area.height.saturating_sub(1),
                ..area
            },
            app.loaded,
            "services",
        );
    }
}

fn render_events(f: &mut Frame, app: &mut App, area: Rect) {
    let mut extra = extras(app, None);
    if app.unacked_only {
        extra.insert(0, "unacked only".to_owned());
    }
    let rows: Vec<Row> = app
        .visible_events()
        .into_iter()
        .map(|e| {
            let subject = match (&e.host, &e.service) {
                (Some(h), Some(s)) => format!("{h}/{s}"),
                (Some(h), None) => h.clone(),
                _ => "-".to_owned(),
            };
            Row::new([
                Cell::from(Span::styled(e.id.to_string(), dim())),
                Cell::from(Span::styled(fmt::clock(e.created_at), dim())),
                Cell::from(Span::styled(
                    event_badge(e.state),
                    style(event_tone(e.state)),
                )),
                Cell::from(Span::styled(subject, Style::new().bold())),
                Cell::from(e.message.clone()),
                Cell::from(Span::styled(if e.acked() { "✔ acked" } else { "" }, dim())),
            ])
        })
        .collect();
    let (shown, total) = (rows.len(), app.events.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(26),
            Constraint::Min(16),
            Constraint::Length(7),
        ],
    )
    .header(header_row(&[
        "ID", "TIME", "STATE", "SUBJECT", "MESSAGE", "",
    ]))
    .block(table_block("Events", shown, total, &extra))
    .row_highlight_style(selected_style())
    .highlight_symbol("▌")
    .highlight_spacing(HighlightSpacing::Always);
    app.events_cursor.sync(shown);
    f.render_stateful_widget(table, area, &mut app.events_cursor.state);
    if shown == 0 {
        empty_hint(
            f,
            Rect {
                y: area.y + 1,
                height: area.height.saturating_sub(1),
                ..area
            },
            app.loaded,
            "events",
        );
    }
}

fn render_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let services: Vec<Row> = app.detail_services().into_iter().map(service_row).collect();
    let Some(detail) = app.detail.as_mut() else {
        return;
    };
    let [info, list] = Layout::vertical([Constraint::Length(10), Constraint::Min(0)]).areas(area);
    f.render_widget(
        Paragraph::new(info_lines(detail)).block(block(title(&detail.name))),
        info,
    );

    let count = services.len();
    let table = Table::new(
        services,
        [
            Constraint::Length(15),
            Constraint::Length(26),
            Constraint::Length(11),
            Constraint::Min(16),
            Constraint::Length(12),
        ],
    )
    .header(header_row(&["STATE", "SERVICE", "TYPE", "STATUS", "SINCE"]))
    .block(block(title(&format!("Services · {count}"))))
    .row_highlight_style(selected_style())
    .highlight_symbol("▌")
    .highlight_spacing(HighlightSpacing::Always);
    detail.cursor.sync(count);
    f.render_stateful_widget(table, list, &mut detail.cursor.state);
}

fn service_row(s: &Service) -> Row<'static> {
    let status = match &s.pending_action {
        Some(action) => format!("{} (pending {action})", s.status_text),
        None => s.status_text.clone(),
    };
    Row::new([
        badge_cell(s.state, false),
        Cell::from(Span::styled(s.name.clone(), Style::new().bold())),
        Cell::from(Span::styled(s.kind.clone(), dim())),
        Cell::from(status),
        Cell::from(Span::styled(fmt::ago(s.state_since), dim())),
    ])
}

fn info_lines(detail: &Detail) -> Vec<Line<'static>> {
    if let Some(error) = &detail.error {
        return vec![Line::from(Span::styled(
            format!(" {error}"),
            style(Tone::Bad),
        ))];
    }
    let Some(data) = &detail.data else {
        return vec![Line::from(Span::styled(" loading…", dim()))];
    };
    let h = &data.summary;
    let field = |name: &str, value: String| {
        Line::from(vec![
            Span::styled(format!(" {name:<10}"), dim()),
            Span::raw(value),
        ])
    };
    let os = [&h.os.name, &h.os.release, &h.os.machine]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!(" {:<10}", "state"), dim()),
            Span::styled(host_badge(h.state), style(host_tone(h.state))),
            Span::styled(format!("   {}", h.hostname), dim()),
        ]),
        field("os", if os.is_empty() { "-".to_owned() } else { os }),
        field(
            "seen",
            format!(
                "{} ago · up {}",
                short_duration(fmt::now() - h.last_seen),
                h.system
                    .uptime
                    .map_or_else(|| "-".to_owned(), short_duration)
            ),
        ),
        Line::from(vec![
            Span::styled(format!(" {:<10}", "cpu"), dim()),
            Span::styled(
                fmt::sparkline(&h.sparkline.cpu, 20),
                Style::new().fg(ACCENT),
            ),
            Span::raw(format!(" {}", fmt::percent(h.system.cpu))),
        ]),
        Line::from(vec![
            Span::styled(format!(" {:<10}", "memory"), dim()),
            Span::styled(fmt::sparkline(&h.sparkline.mem, 20), Style::new().fg(MEM)),
            Span::raw(format!(" {}", fmt::percent(h.system.mem_percent))),
        ]),
        field("load", fmt::load(h.system.load)),
    ];
    let monit = match (&data.monit_url, h.can_act) {
        (Some(url), true) => url.clone(),
        (Some(url), false) => format!("{url} (cannot act)"),
        (None, _) => "unknown".to_owned(),
    };
    lines.push(field("monit", monit));
    lines.push(match &h.muted_until {
        Some(MutedUntil::Forever(_)) => field("muted", "until unmuted".to_owned()),
        Some(MutedUntil::Until(ts)) => field("muted", format!("until {}", fmt::clock(*ts))),
        None => field("muted", "no".to_owned()),
    });
    lines
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(row);
    cell
}

fn render_confirm(f: &mut Frame, pending: &Pending, area: Rect) {
    let prompt = pending.prompt();
    let width = (prompt.chars().count() as u16 + 6).max(36);
    let rect = centered(area, width, 6);
    f.render_widget(Clear, rect);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(prompt, Style::new().bold())).alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::new().fg(Color::Green).bold()),
            Span::styled(" confirm    ", dim()),
            Span::styled("n", Style::new().fg(Color::Red).bold()),
            Span::styled(" cancel", dim()),
        ])
        .alignment(Alignment::Center),
    ];
    let popup = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style(Tone::Warn))
        .title(Span::styled(" Confirm ", style(Tone::Warn).bold()));
    f.render_widget(Paragraph::new(lines).block(popup), rect);
}

fn render_help(f: &mut Frame, area: Rect) {
    const KEYS: &[(&str, &str)] = &[
        ("Tab / 1-4", "switch view (also h/l, arrows)"),
        ("j/k, ↑/↓", "move; g/G first/last; ^d/^u page"),
        ("Enter", "open the host (Hosts, Services, Events)"),
        ("Esc, q", "back; q quits at the top level"),
        ("/", "search the current list; Esc clears"),
        ("f", "cycle the state filter (Hosts, Services)"),
        ("u", "only unacknowledged events"),
        ("a / A", "acknowledge the event / all listed"),
        ("r", "restart the selected service"),
        ("s / S", "start / stop the selected service"),
        ("m", "monitor or unmonitor the selected service"),
        ("R", "remonitor the selected service"),
        ("^r, F5", "refresh now"),
        ("?", "this help"),
        ("^c", "quit"),
    ];
    let rect = centered(area, 68, KEYS.len() as u16 + 4);
    f.render_widget(Clear, rect);
    let mut lines = vec![Line::from("")];
    lines.extend(KEYS.iter().map(|(key, what)| {
        Line::from(vec![
            Span::styled(format!("  {key:<12}"), Style::new().fg(ACCENT).bold()),
            Span::raw((*what).to_owned()),
        ])
    }));
    lines.push(Line::from(Span::styled(
        "  Service actions work in the Services view and on a host page.",
        dim(),
    )));
    f.render_widget(
        Paragraph::new(lines).block(block(title("Keys")).border_style(Style::new().fg(ACCENT))),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::tui::app::Detail as TestDetail;
    use crate::tui::app::tests::{app, host};
    use crate::types::HostDetail;

    fn draw(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_view_renders_without_panicking() {
        let (mut app, _rx) = app();
        for tab in Tab::ALL {
            app.tab = tab;
            for (w, h) in [(120, 30), (80, 24), (40, 10), (10, 4)] {
                let _ = draw(&mut app, w, h);
            }
        }
    }

    #[test]
    fn hosts_view_shows_badges_and_names() {
        let (mut app, _rx) = app();
        app.tab = Tab::Hosts;
        let screen = draw(&mut app, 120, 20);
        assert!(screen.contains("✘ offline"), "{screen}");
        assert!(screen.contains("▲ degraded"), "{screen}");
        assert!(screen.contains("web-1"), "{screen}");
        assert!(screen.contains("● 1 "), "{screen}");
    }

    #[test]
    fn confirm_popup_and_help_overlay() {
        let (mut app, _rx) = app();
        app.tab = Tab::Services;
        app.mode = Mode::Confirm(Pending::Ack(vec![1, 2]));
        assert!(draw(&mut app, 100, 24).contains("Acknowledge 2 event(s)?"));
        app.mode = Mode::Help;
        assert!(draw(&mut app, 100, 30).contains("Keys"));
    }

    #[test]
    fn detail_lists_services() {
        let (mut app, _rx) = app();
        let mut data = HostDetail {
            summary: host(1, "web-1", HostState::Ok),
            ..Default::default()
        };
        data.services = vec![crate::types::Service {
            name: "nginx".into(),
            ..Default::default()
        }];
        app.detail = Some(TestDetail::for_test(1, "web-1", data));
        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("nginx"), "{screen}");
    }
}
