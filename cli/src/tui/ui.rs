//! Rendering of the four views, the header, the footer and the popups.
//! Reads [`App`] and draws; it also records where lists were drawn
//! ([`App::hits`]) so mouse clicks can be mapped back, and ratatui updates
//! the table scroll offsets while rendering.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, HighlightSpacing, Paragraph, Row, Sparkline, Table,
};
use unicode_width::UnicodeWidthStr;

use super::app::{
    App, AttentionKind, Conn, Cursor, Hits, ListHit, ListId, Mode, Page, Pending, Tab,
};
use super::pages;
use crate::fmt::{
    self, Tone, event_badge, event_tone, host_badge, host_tone, service_badge, service_tone,
    short_duration,
};
use crate::types::{Event, HostState, HostSummary, Overview, ServiceListItem, ServiceState};

pub const ACCENT: Color = Color::Cyan;
pub const SELECT_BG: Color = Color::Rgb(38, 44, 66);
pub const DIM: Color = Color::Rgb(128, 132, 148);
pub const MEM: Color = Color::Magenta;

pub fn style(tone: Tone) -> Style {
    match tone {
        Tone::Plain => Style::new(),
        Tone::Good => Style::new().fg(Color::Green),
        Tone::Warn => Style::new().fg(Color::Yellow),
        Tone::Bad => Style::new().fg(Color::Red),
        Tone::Dim => Style::new().fg(DIM),
        Tone::Bold => Style::new().add_modifier(Modifier::BOLD),
    }
}

pub fn dim() -> Style {
    style(Tone::Dim)
}

pub fn block(title: impl Into<Line<'static>>) -> Block<'static> {
    block_focus(title, false)
}

/// A rounded box; the focused one has an accent border.
pub fn block_focus(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    let border = if focused {
        Style::new().fg(ACCENT)
    } else {
        dim()
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(title.into())
}

pub fn title(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {text} "),
        Style::new().fg(ACCENT).bold(),
    ))
}

pub fn selected_style() -> Style {
    Style::new().bg(SELECT_BG).add_modifier(Modifier::BOLD)
}

pub fn render(f: &mut Frame, app: &mut App) {
    app.hits = Hits::default();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    render_header(f, app, header);
    if app.pages.is_empty() {
        match app.tab {
            Tab::Overview => render_overview(f, app, body),
            Tab::Hosts => render_hosts(f, app, body),
            Tab::Services => render_services(f, app, body),
            Tab::Events => render_events(f, app, body),
        }
    } else {
        pages::render(f, app, body);
    }
    render_footer(f, app, footer);

    match &app.mode {
        Mode::Confirm(pending) => render_confirm(f, pending, body),
        Mode::Help => render_help(f, body),
        Mode::Normal | Mode::Search => {}
    }
}

/// One line: brand, tabs, (URL), host counts and the connection state.
fn render_header(f: &mut Frame, app: &mut App, area: Rect) {
    let failed = app.overview.services.failed;
    let unacked = app.unacked_count();
    let full = [
        "Overview".to_owned(),
        format!("Hosts ({})", app.hosts.len()),
        if failed > 0 {
            format!("Services ({failed} failed)")
        } else {
            format!("Services ({})", app.services.len())
        },
        if unacked > 0 {
            format!("Events ({unacked} new)")
        } else {
            "Events".to_owned()
        },
    ];
    let short = ["Overview", "Hosts", "Services", "Events"].map(str::to_owned);

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
    let mut right = vec![
        Span::styled(format!("● {online} "), style(Tone::Good)),
        Span::styled(format!("▲ {degraded} "), style(Tone::Warn)),
        Span::styled(format!("✘ {offline}"), style(Tone::Bad)),
        Span::styled("  │  ", dim()),
    ];
    if app.read_only() {
        right.push(Span::styled("read-only  ", style(Tone::Warn)));
    }
    right.push(Span::styled(format!("{conn_text} "), style(conn_tone)));
    let right_width: usize = right.iter().map(|s| s.content.width()).sum();

    let brand = " ◆ monarch ";
    let width = usize::from(area.width);
    let tabs_width = |titles: &[String; 4]| {
        titles.iter().map(|t| t.width() + 2).sum::<usize>() + 3 // dividers
    };
    let titles = if brand.width() + tabs_width(&full) + right_width < width {
        full
    } else {
        short
    };

    let mut left = vec![Span::styled(brand, Style::new().fg(ACCENT).bold())];
    let mut x = area.x + brand.width() as u16;
    for (i, (tab, label)) in Tab::ALL.iter().zip(titles.iter()).enumerate() {
        if i > 0 {
            left.push(Span::styled("│", dim()));
            x += 1;
        }
        let text = format!(" {label} ");
        let cells = text.width() as u16;
        let active = *tab == app.tab;
        let tab_style = if active {
            Style::new()
                .fg(ACCENT)
                .bold()
                .add_modifier(Modifier::UNDERLINED)
        } else {
            dim()
        };
        left.push(Span::styled(text, tab_style));
        app.hits.tabs.push((*tab, Rect::new(x, area.y, cells, 1)));
        x += cells;
    }
    let left_width = usize::from(x - area.x);
    let url = format!("  {}", app.base_url);
    if left_width + url.width() + right_width < width {
        left.push(Span::styled(url, dim()));
    }
    f.render_widget(Paragraph::new(Line::from(left)), area);
    f.render_widget(
        Paragraph::new(Line::from(right)).alignment(Alignment::Right),
        area,
    );
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
    let hints: &[(&'static str, &'static str)] = match app.pages.last() {
        Some(Page::Host(_)) => &[
            ("j/k", "move"),
            ("l/Enter", "service"),
            ("h/Esc", "back"),
            ("t", "range"),
            ("r", "restart"),
            ("m", "monitor"),
            ("?", "help"),
        ],
        Some(Page::Service(_)) => &[
            ("j/k", "event"),
            ("l/Enter", "open"),
            ("h/Esc", "back"),
            ("t", "range"),
            ("r", "restart"),
            ("s/S", "start/stop"),
            ("m", "monitor"),
            ("?", "help"),
        ],
        Some(Page::Event(_)) => &[
            ("l/Enter", "go to service"),
            ("h/Esc", "back"),
            ("a", "ack"),
            ("?", "help"),
        ],
        None => match app.tab {
            Tab::Overview => &[
                ("p", "pane"),
                ("j/k", "move"),
                ("l/Enter", "open"),
                ("Tab", "next view"),
                ("^r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Tab::Hosts => &[
                ("j/k", "move"),
                ("l/Enter", "open"),
                ("/", "search"),
                ("f", "state"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Tab::Services => &[
                ("j/k", "move"),
                ("l/Enter", "open"),
                ("/", "search"),
                ("f", "state"),
                ("r", "restart"),
                ("m", "monitor"),
                ("?", "help"),
                ("q", "quit"),
            ],
            Tab::Events => &[
                ("j/k", "move"),
                ("l/Enter", "open"),
                ("a", "ack"),
                ("A", "ack all"),
                ("u", "unacked"),
                ("/", "search"),
                ("?", "help"),
                ("q", "quit"),
            ],
        },
    };
    let mut spans = vec![Span::raw(" ")];
    for (key, what) in hints {
        spans.extend(hint(key, what));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn header_row(columns: &[&'static str]) -> Row<'static> {
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

pub fn empty_hint(f: &mut Frame, area: Rect, loaded: bool, what: &str) {
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

/// Renders a selectable table and records it for mouse hit testing.
#[allow(clippy::too_many_arguments)]
pub fn render_list(
    f: &mut Frame,
    hits: &mut Hits,
    id: ListId,
    area: Rect,
    block: Block<'static>,
    header: Option<Row<'static>>,
    rows: Vec<Row<'static>>,
    widths: &[Constraint],
    cursor: &mut Cursor,
    focused: bool,
) {
    let len = rows.len();
    let has_header = header.is_some();
    let mut table = Table::new(rows, widths.to_vec())
        .block(block)
        .row_highlight_style(if focused {
            selected_style()
        } else {
            Style::new()
        })
        .highlight_symbol(if focused { "▌" } else { " " })
        .highlight_spacing(HighlightSpacing::Always);
    if let Some(header) = header {
        table = table.header(header);
    }
    cursor.sync(len);
    f.render_stateful_widget(table, area, &mut cursor.state);
    hits.lists.push(ListHit {
        id,
        outer: area,
        header: has_header,
    });
}

fn badge_cell(state: ServiceState, stale: bool) -> Cell<'static> {
    if stale {
        Cell::from(Span::styled("… stale", dim()))
    } else {
        Cell::from(Span::styled(
            service_badge(state),
            style(service_tone(state)),
        ))
    }
}

pub fn service_status(status_text: &str, pending: Option<&str>) -> String {
    match pending {
        Some(action) => format!("{status_text} (pending {action})"),
        None => status_text.to_owned(),
    }
}

pub fn state_cell(state: ServiceState) -> Cell<'static> {
    badge_cell(state, false)
}

fn subject(e: &Event) -> String {
    match (&e.host, &e.service) {
        (Some(h), Some(s)) => format!("{h}/{s}"),
        (Some(h), None) => h.clone(),
        _ => "-".to_owned(),
    }
}

// ---- overview ----------------------------------------------------------

fn count_span(symbol: &str, label: &str, value: u64, tone: Tone) -> Span<'static> {
    let tone = if value == 0 { Tone::Dim } else { tone };
    Span::styled(format!("{symbol} {value} {label}   "), style(tone))
}

fn render_overview(f: &mut Frame, app: &mut App, area: Rect) {
    let top_height = match area.height {
        30.. => 10,
        22.. => 8,
        _ => 6,
    }
    .min(area.height / 2);
    let [top, bottom] =
        Layout::vertical([Constraint::Length(top_height), Constraint::Min(0)]).areas(area);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(top);
    let [hosts_area, services_area] =
        Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(left);
    let o = &app.overview;

    let hosts = vec![
        Line::from(vec![
            count_span(
                "●",
                "online",
                o.hosts
                    .total
                    .saturating_sub(o.hosts.degraded + o.hosts.offline),
                Tone::Good,
            ),
            count_span("▲", "degraded", o.hosts.degraded, Tone::Warn),
            count_span("✘", "offline", o.hosts.offline, Tone::Bad),
        ]),
        Line::from(Span::styled(format!("{} in total", o.hosts.total), dim())),
    ];
    f.render_widget(
        Paragraph::new(hosts).block(block(title("Hosts"))),
        hosts_area,
    );

    let services = vec![
        Line::from(vec![
            count_span("✔", "ok", o.services.ok, Tone::Good),
            count_span("✘", "failed", o.services.failed, Tone::Bad),
            count_span("…", "pending", o.services.pending, Tone::Warn),
            count_span("○", "unmonitored", o.services.unmonitored, Tone::Dim),
        ]),
        Line::from(Span::styled(
            format!("{} in total", o.services.total),
            dim(),
        )),
    ];
    f.render_widget(
        Paragraph::new(services).block(block(title("Services"))),
        services_area,
    );
    render_activity(f, o, right);

    let [attention_area, events_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(bottom);

    let items = app.attention();
    let rows: Vec<Row> = items
        .iter()
        .map(|a| {
            let (badge, tone) = match a.kind {
                AttentionKind::Offline => ("✘ offline", Tone::Bad),
                AttentionKind::Degraded => ("▲ degraded", Tone::Warn),
                AttentionKind::Failed => ("✘ failed", Tone::Bad),
                AttentionKind::Unmonitored => ("○ unmonitored", Tone::Dim),
            };
            let subject = match &a.service {
                Some(service) => format!("{}/{service}", a.host),
                None => a.host.clone(),
            };
            let detail = match a.kind {
                AttentionKind::Offline => format!("last seen {}", fmt::ago(a.since)),
                AttentionKind::Failed => format!("{}, {}", a.detail, fmt::ago(a.since)),
                _ => a.detail.clone(),
            };
            Row::new([
                Cell::from(Span::styled(badge, style(tone))),
                Cell::from(Span::styled(subject, Style::new().bold())),
                Cell::from(Span::styled(detail, dim())),
            ])
        })
        .collect();
    let attention_title = if items.is_empty() {
        "Needs attention".to_owned()
    } else {
        format!("Needs attention · {}", items.len())
    };
    let focus = app.overview_focus;
    let empty = items.is_empty();
    render_list(
        f,
        &mut app.hits,
        ListId::Attention,
        attention_area,
        block_focus(title(&attention_title), focus == ListId::Attention),
        None,
        rows,
        &[
            Constraint::Length(14),
            Constraint::Percentage(45),
            Constraint::Min(8),
        ],
        &mut app.attention_cursor,
        focus == ListId::Attention,
    );
    if empty {
        let text = if app.loaded {
            "✔ All systems operational"
        } else {
            "loading…"
        };
        let tone = if app.loaded { Tone::Good } else { Tone::Dim };
        let inner = block("").inner(attention_area);
        f.render_widget(
            Paragraph::new(Span::styled(format!(" {text}"), style(tone))),
            inner,
        );
    }

    let rows: Vec<Row> = app
        .events
        .iter()
        .map(|e| {
            Row::new([
                Cell::from(Span::styled(
                    format!("{:>4}", short_duration(fmt::now() - e.created_at)),
                    dim(),
                )),
                Cell::from(Span::styled(
                    event_badge(e.state)
                        .chars()
                        .next()
                        .unwrap_or('?')
                        .to_string(),
                    style(event_tone(e.state)),
                )),
                Cell::from(Span::styled(subject(e), Style::new().bold())),
                Cell::from(Span::styled(e.message.clone(), dim())),
            ])
        })
        .collect();
    let events_title = format!("Recent events · {}", rows.len());
    let empty = rows.is_empty();
    render_list(
        f,
        &mut app.hits,
        ListId::OverviewEvents,
        events_area,
        block_focus(title(&events_title), focus == ListId::OverviewEvents),
        None,
        rows,
        &[
            Constraint::Length(5),
            Constraint::Length(2),
            Constraint::Percentage(40),
            Constraint::Min(8),
        ],
        &mut app.overview_events_cursor,
        focus == ListId::OverviewEvents,
    );
    if empty {
        let inner = block("").inner(events_area);
        f.render_widget(Paragraph::new(Span::styled(" no events yet", dim())), inner);
    }
}

/// Events of the last 24 hours as two stacked bar charts: succeeded above, failed below.
fn render_activity(f: &mut Frame, o: &Overview, area: Rect) {
    let failed: u64 = o.activity.iter().map(|b| b.failed).sum();
    let heading = format!("Events, 24h · {} events · {failed} failed", o.events_24h);
    let frame = block(title(&heading));
    let inner = frame.inner(area);
    f.render_widget(frame, area);
    if inner.height < 3 || inner.width < 20 || o.activity.is_empty() {
        return;
    }
    let [plot, axis] = Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).areas(inner);
    let [ok_area, failed_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(plot);

    let gutter = 8;
    let mut chart_width = 0;
    for (area, label, values, tone) in [
        (
            ok_area,
            "ok",
            o.activity.iter().map(|b| b.ok).collect::<Vec<_>>(),
            Tone::Good,
        ),
        (
            failed_area,
            "failed",
            o.activity.iter().map(|b| b.failed).collect(),
            Tone::Bad,
        ),
    ] {
        let [labels, chart] =
            Layout::horizontal([Constraint::Length(gutter), Constraint::Min(1)]).areas(area);
        chart_width = chart.width;
        let max = values.iter().copied().max().unwrap_or(0);
        let lines = vec![
            Line::from(Span::styled(format!(" {label}"), style(tone).bold())),
            Line::from(Span::styled(format!(" max {max}"), dim())),
        ];
        f.render_widget(Paragraph::new(lines), labels);
        // Stretch each hourly bucket over several columns so the chart fills the width.
        let stretch = (usize::from(chart.width) / values.len()).max(1);
        let data: Vec<u64> = values
            .iter()
            .flat_map(|v| std::iter::repeat_n(*v, stretch))
            .collect();
        f.render_widget(
            Sparkline::default()
                .data(data)
                .max(max.max(1))
                .style(style(tone)),
            chart,
        );
    }
    let width = usize::from(chart_width);
    let middle = width.saturating_sub(7);
    let axis_line = format!(
        "{:gutter$}{:<4}{:^middle$}{:>3}",
        "",
        "-24h",
        "-12h",
        "now",
        gutter = usize::from(gutter)
    );
    f.render_widget(Paragraph::new(Span::styled(axis_line, dim())), axis);
}

// ---- lists --------------------------------------------------------------

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

fn render_hosts(f: &mut Frame, app: &mut App, area: Rect) {
    let extra = extras(app, Some(app.host_filter.label()));
    let rows: Vec<Row> = app.visible_hosts().into_iter().map(host_row).collect();
    let (shown, total) = (rows.len(), app.hosts.len());
    render_list(
        f,
        &mut app.hits,
        ListId::Hosts,
        area,
        table_block("Hosts", shown, total, &extra),
        Some(header_row(&[
            "STATE", "HOST", "CPU", "MEM", "LOAD", "SERVICES", "SEEN",
        ])),
        rows,
        &[
            Constraint::Length(12),
            Constraint::Min(16),
            Constraint::Length(17),
            Constraint::Length(17),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(8),
        ],
        &mut app.hosts_cursor,
        true,
    );
    if shown == 0 {
        empty_hint(f, below_header(area), app.loaded, "hosts");
    }
}

fn below_header(area: Rect) -> Rect {
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
}

fn render_services(f: &mut Frame, app: &mut App, area: Rect) {
    let extra = extras(app, Some(app.service_filter.label()));
    let rows: Vec<Row> = app
        .visible_services()
        .into_iter()
        .map(|s: &ServiceListItem| {
            Row::new([
                badge_cell(s.state, s.stale),
                Cell::from(s.host.clone()),
                Cell::from(Span::styled(s.name.clone(), Style::new().bold())),
                Cell::from(Span::styled(s.kind.clone(), dim())),
                Cell::from(service_status(&s.status_text, s.pending_action.as_deref())),
                Cell::from(Span::styled(fmt::ago(s.state_since), dim())),
            ])
        })
        .collect();
    let (shown, total) = (rows.len(), app.services.len());
    render_list(
        f,
        &mut app.hits,
        ListId::Services,
        area,
        table_block("Services", shown, total, &extra),
        Some(header_row(&[
            "STATE", "HOST", "SERVICE", "TYPE", "STATUS", "SINCE",
        ])),
        rows,
        &[
            Constraint::Length(15),
            Constraint::Length(20),
            Constraint::Length(22),
            Constraint::Length(11),
            Constraint::Min(16),
            Constraint::Length(12),
        ],
        &mut app.services_cursor,
        true,
    );
    if shown == 0 {
        empty_hint(f, below_header(area), app.loaded, "services");
    }
}

pub fn event_row(e: &Event) -> Row<'static> {
    Row::new([
        Cell::from(Span::styled(e.id.to_string(), dim())),
        Cell::from(Span::styled(fmt::clock(e.created_at), dim())),
        Cell::from(Span::styled(
            event_badge(e.state),
            style(event_tone(e.state)),
        )),
        Cell::from(Span::styled(subject(e), Style::new().bold())),
        Cell::from(e.message.clone()),
        Cell::from(Span::styled(if e.acked() { "✔ acked" } else { "" }, dim())),
    ])
}

pub const EVENT_WIDTHS: [Constraint; 6] = [
    Constraint::Length(6),
    Constraint::Length(14),
    Constraint::Length(12),
    Constraint::Length(26),
    Constraint::Min(16),
    Constraint::Length(7),
];

pub fn event_header() -> Row<'static> {
    header_row(&["ID", "TIME", "STATE", "SUBJECT", "MESSAGE", ""])
}

fn render_events(f: &mut Frame, app: &mut App, area: Rect) {
    let mut extra = extras(app, None);
    if app.unacked_only {
        extra.insert(0, "unacked only".to_owned());
    }
    let rows: Vec<Row> = app.visible_events().into_iter().map(event_row).collect();
    let (shown, total) = (rows.len(), app.events.len());
    render_list(
        f,
        &mut app.hits,
        ListId::Events,
        area,
        table_block("Events", shown, total, &extra),
        Some(event_header()),
        rows,
        &EVENT_WIDTHS,
        &mut app.events_cursor,
        true,
    );
    if shown == 0 {
        empty_hint(f, below_header(area), app.loaded, "events");
    }
}

// ---- popups --------------------------------------------------------------

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
        ("Tab, 1-4", "switch view (Shift-Tab goes the other way)"),
        ("j/k, ↑/↓", "move; g/G first/last; ^d/^u page"),
        ("l, →, Enter", "open the selected entry (drill in)"),
        ("h, ←, Esc, q", "go back; q quits at the top level"),
        ("p", "Overview: focus the other list"),
        ("/", "search the current list; Esc clears"),
        ("f", "cycle the state filter (Hosts, Services)"),
        ("u", "only unacknowledged events"),
        ("a / A", "acknowledge the event / all listed"),
        ("r", "restart the selected service"),
        ("s / S", "start / stop the selected service"),
        ("m", "monitor or unmonitor the selected service"),
        ("R", "remonitor the selected service"),
        ("t", "chart range: 1h, 6h, 24h, 7d"),
        ("^r, F5", "refresh now"),
        ("mouse", "click tabs and rows, double-click to open,"),
        ("", "wheel scrolls, right-click goes back"),
        ("?", "this help"),
        ("^c", "quit"),
    ];
    let rect = centered(area, 72, KEYS.len() as u16 + 4);
    f.render_widget(Clear, rect);
    let mut lines = vec![Line::from("")];
    lines.extend(KEYS.iter().map(|(key, what)| {
        Line::from(vec![
            Span::styled(format!("  {key:<14}"), Style::new().fg(ACCENT).bold()),
            Span::raw((*what).to_owned()),
        ])
    }));
    lines.push(Line::from(Span::styled(
        "  Drilling in stacks pages: going back returns to the exact previous view.",
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
    use ratatui::crossterm::event::{
        Event as TermEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };

    use super::*;
    use crate::tui::app::tests::{app, feed_host, host, key, press};
    use crate::tui::app::{HostPage, Msg};
    use crate::types::HostDetail;

    pub fn draw(app: &mut App, width: u16, height: u16) -> String {
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

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Msg {
        Msg::Input(TermEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
    }

    fn click(app: &mut App, column: u16, row: u16) {
        app.update(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    }

    #[test]
    fn every_view_renders_without_panicking() {
        let (mut app, _rx) = app();
        for tab in Tab::ALL {
            app.tab = tab;
            for (w, h) in [(120, 40), (120, 30), (80, 24), (40, 10), (10, 4)] {
                let _ = draw(&mut app, w, h);
            }
        }
    }

    #[test]
    fn header_is_one_line_with_brand_tabs_counts_and_state() {
        let (mut app, _rx) = app();
        let screen = draw(&mut app, 130, 30);
        let first = screen.lines().next().unwrap();
        for needle in [
            "monarch",
            "Overview",
            "Hosts (3)",
            "Services",
            "Events",
            "● 1",
            "▲ 1",
            "✘ 1",
            "connecting",
        ] {
            assert!(first.contains(needle), "header lacks {needle}: {first}");
        }
        assert!(
            first.contains("127.0.0.1"),
            "wide terminals show the URL: {first}"
        );
        let narrow = draw(&mut app, 70, 20);
        assert!(narrow.lines().next().unwrap().contains("Overview"));
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
    fn overview_shows_both_lists_and_the_focus() {
        let (mut app, _rx) = app();
        let screen = draw(&mut app, 130, 30);
        for needle in [
            "Needs attention · 3",
            "Recent events · 2",
            "Events, 24h",
            "db-1",
            "sshd is down",
            "p pane",
        ] {
            assert!(screen.contains(needle), "missing {needle}:\n{screen}");
        }
        assert!(screen.contains('▌'), "the focused list marks its selection");
    }

    #[test]
    fn confirm_popup_and_help_overlay() {
        let (mut app, _rx) = app();
        app.tab = Tab::Services;
        app.mode = Mode::Confirm(Pending::Ack(vec![1, 2]));
        assert!(draw(&mut app, 100, 24).contains("Acknowledge 2 event(s)?"));
        app.mode = Mode::Help;
        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("Keys"));
        assert!(screen.contains("right-click"));
    }

    #[tokio::test]
    async fn host_page_lists_services_and_draws_charts() {
        let (mut app, _rx) = app();
        let mut data = HostDetail {
            summary: host(1, "web-1", HostState::Ok),
            ..Default::default()
        };
        data.services = vec![crate::types::Service {
            name: "nginx".into(),
            ..Default::default()
        }];
        app.pages
            .push(Page::Host(Box::new(HostPage::for_test(1, "web-1", data))));
        let screen = draw(&mut app, 120, 40);
        assert!(screen.contains("nginx"), "{screen}");
        assert!(screen.contains("h/Esc back"), "{screen}");
    }

    #[tokio::test]
    async fn mouse_clicks_select_and_open() {
        let (mut app, _rx) = app();
        draw(&mut app, 130, 30);

        // clicking a tab switches the view
        let (_, rect) = *app
            .hits
            .tabs
            .iter()
            .find(|(t, _)| *t == Tab::Hosts)
            .unwrap();
        click(&mut app, rect.x + 1, rect.y);
        assert_eq!(app.tab, Tab::Hosts);
        draw(&mut app, 130, 30);

        // clicking a row selects it, clicking it again opens it
        let hit = app
            .hits
            .lists
            .iter()
            .find(|h| h.id == ListId::Hosts)
            .copied()
            .unwrap();
        let second_row = hit.outer.y + 2 + 1; // border, header, then the second host
        click(&mut app, hit.outer.x + 4, second_row);
        assert_eq!(app.hosts_cursor.selected(), 1);
        assert!(app.pages.is_empty());
        click(&mut app, hit.outer.x + 4, second_row);
        assert!(matches!(app.pages.last(), Some(Page::Host(_))));

        // right-click goes back
        app.update(mouse(MouseEventKind::Down(MouseButton::Right), 5, 5));
        assert!(app.pages.is_empty());
    }

    #[tokio::test]
    async fn mouse_focuses_overview_panes_and_scrolls() {
        let (mut app, _rx) = app();
        draw(&mut app, 130, 30);
        let events = app
            .hits
            .lists
            .iter()
            .find(|h| h.id == ListId::OverviewEvents)
            .copied()
            .unwrap();
        click(&mut app, events.outer.x + 3, events.outer.y + 1);
        assert_eq!(app.overview_focus, ListId::OverviewEvents);
        draw(&mut app, 130, 30);
        app.update(mouse(
            MouseEventKind::ScrollDown,
            events.outer.x + 3,
            events.outer.y + 2,
        ));
        assert_eq!(app.overview_events_cursor.selected(), 1);
        app.update(mouse(
            MouseEventKind::ScrollUp,
            events.outer.x + 3,
            events.outer.y + 2,
        ));
        assert_eq!(app.overview_events_cursor.selected(), 0);
        // wheel over the other pane scrolls (and focuses) that one
        let attention = app
            .hits
            .lists
            .iter()
            .find(|h| h.id == ListId::Attention)
            .copied()
            .unwrap();
        app.update(mouse(
            MouseEventKind::ScrollDown,
            attention.outer.x + 2,
            attention.outer.y + 2,
        ));
        assert_eq!(app.overview_focus, ListId::Attention);
        assert_eq!(app.attention_cursor.selected(), 1);
    }

    #[test]
    fn help_closes_on_click_and_confirm_ignores_it() {
        let (mut app, _rx) = app();
        app.mode = Mode::Help;
        click(&mut app, 3, 3);
        assert_eq!(app.mode, Mode::Normal);
        app.mode = Mode::Confirm(Pending::Ack(vec![1]));
        click(&mut app, 3, 3);
        assert!(matches!(app.mode, Mode::Confirm(_)));
    }

    #[tokio::test]
    async fn keys_and_pages_render_consistently() {
        let (mut app, _rx) = app();
        press(&mut app, "2");
        app.update(key(ratatui::crossterm::event::KeyCode::Enter));
        feed_host(&mut app, 2, "db-1");
        let screen = draw(&mut app, 120, 40);
        assert!(screen.contains("db-1"), "{screen}");
    }
}
