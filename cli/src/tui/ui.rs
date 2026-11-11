//! Rendering of the four views, the header, the footer and the popups.
//!
//! Every view adapts to the terminal width ([`Size`]): side by side on wide
//! terminals, stacked on narrow ones, with low-priority table columns dropped
//! and long text cut with `…`. The renderer also records where lists were drawn
//! ([`App::hits`]) so mouse clicks map back to the same layout, and ratatui
//! updates the table scroll offsets while rendering.

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
use super::layout::{self, Col, Planned, Size, Width, ellipsize};
use super::pages;
use crate::fmt::{
    self, Tone, event_badge, event_tone, host_badge, host_tone, service_badge, service_tone,
    short_duration,
};
use crate::types::{Event, HostState, HostSummary, MutedUntil, Overview, ServiceState};

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
    let size = Size::of(f.area().width);
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    render_header(f, app, header, size);
    if app.pages.is_empty() {
        match app.tab {
            Tab::Overview => render_overview(f, app, body, size),
            Tab::Hosts => render_hosts(f, app, body, size),
            Tab::Services => render_services(f, app, body, size),
            Tab::Events => render_events(f, app, body, size),
        }
    } else {
        pages::render(f, app, body, size);
    }
    render_footer(f, app, footer, size);

    match app.mode.clone() {
        Mode::Confirm(pending) => render_confirm(f, &pending, body),
        Mode::Help => render_help(f, app, body),
        Mode::Normal | Mode::Search => {}
    }
}

// ---- header and footer ---------------------------------------------------

/// One line: brand, tabs, (URL), host counts and the connection state.
/// Narrow screens show only the active tab's name and a number for the others.
fn render_header(f: &mut Frame, app: &mut App, area: Rect, size: Size) {
    let count =
        |hosts: &[HostSummary], state: HostState| hosts.iter().filter(|h| h.state == state).count();
    let (offline, degraded) = (
        count(&app.hosts, HostState::Offline),
        count(&app.hosts, HostState::Degraded),
    );
    let online = app.hosts.len() - offline - degraded;
    let (conn_text, conn_dot, conn_tone) = match (&app.api_error, app.conn) {
        (Some(_), _) => ("✘ offline", "✘", Tone::Bad),
        (None, Conn::Live) => ("● live", "●", Tone::Good),
        (None, Conn::Connecting) => ("○ connecting", "○", Tone::Warn),
        (None, Conn::Reconnecting) => ("○ reconnecting", "○", Tone::Warn),
    };
    let width = usize::from(area.width);
    let names = ["Overview", "Hosts", "Services", "Events"];

    if size.narrow() {
        let brand = " ◆";
        let mut left = vec![Span::styled(brand, Style::new().fg(ACCENT).bold())];
        let mut x = area.x + brand.width() as u16;
        for (i, tab) in Tab::ALL.iter().enumerate() {
            let active = *tab == app.tab;
            let text = if active {
                format!(" {}:{} ", i + 1, names[i])
            } else {
                format!(" {} ", i + 1)
            };
            let cells = text.width() as u16;
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
        let mut right = vec![
            Span::styled(format!("●{online} "), style(Tone::Good)),
            Span::styled(format!("▲{degraded} "), style(Tone::Warn)),
            Span::styled(format!("✘{offline} "), style(Tone::Bad)),
        ];
        if app.read_only() {
            right.push(Span::styled("ro ", style(Tone::Warn)));
        }
        right.push(Span::styled(format!("{conn_dot} "), style(conn_tone)));
        let right_width: usize = right.iter().map(|s| s.content.width()).sum();
        if left_width + right_width > width {
            right = vec![Span::styled(format!("{conn_dot} "), style(conn_tone))];
        }
        f.render_widget(Paragraph::new(Line::from(left)), area);
        f.render_widget(
            Paragraph::new(Line::from(right)).alignment(Alignment::Right),
            area,
        );
        return;
    }

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
    let short = names.map(str::to_owned);
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

type Hint = (&'static str, &'static str);

fn hints(app: &App, narrow: bool) -> &'static [Hint] {
    match (app.pages.last(), app.tab, narrow) {
        (Some(Page::Host(_)), _, false) => &[
            ("j/k", "move"),
            ("l/Enter", "service"),
            ("h/Esc", "back"),
            ("t", "range"),
            ("c", "chart"),
            ("r", "restart"),
            ("m", "monitor"),
            ("?", "help"),
        ],
        (Some(Page::Service(_)), _, false) => &[
            ("j/k", "event"),
            ("l/Enter", "open"),
            ("h/Esc", "back"),
            ("t", "range"),
            ("c", "chart"),
            ("r", "restart"),
            ("s/S", "start/stop"),
            ("m", "monitor"),
            ("?", "help"),
        ],
        (Some(Page::Event(_)), _, false) => &[
            ("l/Enter", "go to service"),
            ("h/Esc", "back"),
            ("a", "ack"),
            ("?", "help"),
        ],
        (Some(Page::Host(_) | Page::Service(_)), _, true) => {
            &[("h", "back"), ("l", "open"), ("c", "chart"), ("?", "help")]
        }
        (Some(Page::Event(_)), _, true) => &[("h", "back"), ("l", "go"), ("?", "help")],
        (None, Tab::Overview, false) => &[
            ("p", "pane"),
            ("j/k", "move"),
            ("l/Enter", "open"),
            ("Tab", "next view"),
            ("^r", "refresh"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (None, Tab::Hosts, false) => &[
            ("j/k", "move"),
            ("l/Enter", "open"),
            ("/", "search"),
            ("f", "state"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (None, Tab::Services, false) => &[
            ("j/k", "move"),
            ("l/Enter", "open"),
            ("/", "search"),
            ("f", "state"),
            ("r", "restart"),
            ("m", "monitor"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (None, Tab::Events, false) => &[
            ("j/k", "move"),
            ("l/Enter", "open"),
            ("a", "ack"),
            ("A", "ack all"),
            ("u", "unacked"),
            ("/", "search"),
            ("?", "help"),
            ("q", "quit"),
        ],
        (None, Tab::Overview, true) => {
            &[("p", "pane"), ("l", "open"), ("?", "help"), ("q", "quit")]
        }
        (None, Tab::Hosts | Tab::Services, true) => {
            &[("l", "open"), ("/", "find"), ("?", "help"), ("q", "quit")]
        }
        (None, Tab::Events, true) => &[("l", "open"), ("a", "ack"), ("?", "help"), ("q", "quit")],
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect, size: Size) {
    if app.mode == Mode::Search {
        let line = Line::from(vec![
            Span::styled(" / ", Style::new().fg(ACCENT).bold()),
            Span::raw(app.search_text().to_owned()),
            Span::styled("▏", Style::new().fg(ACCENT)),
            Span::styled(
                if size.narrow() {
                    "  ⏎ ok  Esc clear"
                } else {
                    "    Enter apply   Esc clear"
                },
                dim(),
            ),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    if let Some(toast) = app.toast.as_ref().filter(|t| t.visible()) {
        let line = Line::from(Span::styled(
            format!(
                " {}",
                ellipsize(&toast.text, usize::from(area.width).saturating_sub(2))
            ),
            style(toast.tone).bold(),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    for (key, what) in hints(app, size.narrow()) {
        let width = key.width() + what.width() + 4;
        if used + width > usize::from(area.width) + 2 {
            break;
        }
        used += width;
        spans.push(Span::styled(*key, Style::new().fg(ACCENT)));
        spans.push(Span::styled(format!(" {what}   "), dim()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---- table helpers ---------------------------------------------------------

/// Cells available to columns: the box takes two, the highlight symbol one.
pub fn avail(area: Rect) -> u16 {
    area.width.saturating_sub(3)
}

pub fn header_of<T>(planned: &[Planned<T>]) -> Row<'static> {
    Row::new(planned.iter().map(|p| Cell::from(p.header))).style(dim().bold())
}

pub fn widths_of<T>(planned: &[Planned<T>]) -> Vec<Constraint> {
    planned
        .iter()
        .map(|p| Constraint::Length(p.width))
        .collect()
}

/// A cell whose text is cut to the column width with `…`.
pub fn text(content: &str, width: u16, style: Style) -> Cell<'static> {
    Cell::from(Span::styled(ellipsize(content, usize::from(width)), style))
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

fn below_header(area: Rect) -> Rect {
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
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

pub fn service_status(status_text: &str, pending: Option<&str>) -> String {
    match pending {
        Some(action) => format!("{status_text} (pending {action})"),
        None => status_text.to_owned(),
    }
}

fn subject(e: &Event) -> String {
    match (&e.host, &e.service) {
        (Some(h), Some(s)) => format!("{h}/{s}"),
        (Some(h), None) => h.clone(),
        _ => "-".to_owned(),
    }
}

/// The first character of a badge, for when only a symbol fits.
fn symbol(badge: &str) -> &str {
    badge.chars().next().map_or("", |c| &badge[..c.len_utf8()])
}

fn state_cell(badge: &'static str, tone: Tone, width: u16) -> Cell<'static> {
    let shown = if width < 6 { symbol(badge) } else { badge };
    text(shown, width, style(tone))
}

// ---- service rows (Services tab, host page) ----------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceCol {
    State,
    Host,
    Name,
    Kind,
    Status,
    Since,
}

/// Columns of a service table. Text columns are sized to their longest value
/// (`host_longest` is `None` when the table has no host column).
pub fn service_cols(
    narrow: bool,
    host_longest: Option<usize>,
    name_longest: usize,
) -> Vec<Col<ServiceCol>> {
    let mut cols = vec![Col {
        id: ServiceCol::State,
        header: if narrow { "" } else { "STATE" },
        width: Width::Fixed(if narrow { 2 } else { 15 }),
        priority: 9,
    }];
    if let Some(longest) = host_longest {
        cols.push(Col {
            id: ServiceCol::Host,
            header: "HOST",
            width: Width::content(8, longest, 24),
            priority: 6,
        });
    }
    cols.extend([
        Col {
            id: ServiceCol::Name,
            header: "SERVICE",
            width: Width::content(10, name_longest, 40),
            priority: 10,
        },
        Col {
            id: ServiceCol::Kind,
            header: "TYPE",
            width: Width::Fixed(11),
            priority: 3,
        },
        Col {
            id: ServiceCol::Status,
            header: "STATUS",
            width: Width::flex(12, 2),
            priority: 7,
        },
        Col {
            id: ServiceCol::Since,
            header: "SINCE",
            width: Width::Fixed(8),
            priority: 4,
        },
    ]);
    cols
}

/// What a service row shows, whichever API type it comes from.
pub struct ServiceView<'a> {
    pub state: ServiceState,
    pub stale: bool,
    pub host: &'a str,
    pub name: &'a str,
    pub kind: &'a str,
    pub status: String,
    pub since: Option<f64>,
}

pub fn service_row(planned: &[Planned<ServiceCol>], s: &ServiceView) -> Row<'static> {
    Row::new(planned.iter().map(|p| match p.id {
        ServiceCol::State if s.stale => text("… stale", p.width, dim()),
        ServiceCol::State => state_cell(service_badge(s.state), service_tone(s.state), p.width),
        ServiceCol::Host => text(s.host, p.width, Style::new()),
        ServiceCol::Name => text(s.name, p.width, Style::new().bold()),
        ServiceCol::Kind => text(s.kind, p.width, dim()),
        ServiceCol::Status => text(&s.status, p.width, Style::new()),
        ServiceCol::Since => text(&fmt::ago(s.since), p.width, dim()),
    }))
}

// ---- event rows ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCol {
    Id,
    Time,
    State,
    Subject,
    Message,
    Ack,
}

pub fn event_cols(narrow: bool, subject_longest: usize) -> Vec<Col<EventCol>> {
    vec![
        Col {
            id: EventCol::Id,
            header: "ID",
            width: Width::Fixed(6),
            priority: 3,
        },
        Col {
            id: EventCol::Time,
            header: if narrow { "AGE" } else { "TIME" },
            width: Width::Fixed(if narrow { 4 } else { 14 }),
            priority: 5,
        },
        Col {
            id: EventCol::State,
            header: if narrow { "" } else { "STATE" },
            width: Width::Fixed(if narrow { 2 } else { 12 }),
            priority: 9,
        },
        Col {
            id: EventCol::Subject,
            header: "SUBJECT",
            width: Width::content(12, subject_longest, 36),
            priority: 8,
        },
        Col {
            id: EventCol::Message,
            header: "MESSAGE",
            width: Width::flex(12, 2),
            priority: 10,
        },
        Col {
            id: EventCol::Ack,
            header: "",
            width: Width::Fixed(if narrow { 1 } else { 7 }),
            priority: 6,
        },
    ]
}

pub fn longest_subject<'a>(events: impl IntoIterator<Item = &'a Event>) -> usize {
    events
        .into_iter()
        .map(|e| subject(e).width())
        .max()
        .unwrap_or(0)
}

pub fn event_row(planned: &[Planned<EventCol>], e: &Event, narrow: bool) -> Row<'static> {
    Row::new(planned.iter().map(|p| match p.id {
        EventCol::Id => text(&e.id.to_string(), p.width, dim()),
        EventCol::Time if narrow => {
            text(&short_duration(fmt::now() - e.created_at), p.width, dim())
        }
        EventCol::Time => text(&fmt::clock(e.created_at), p.width, dim()),
        EventCol::State => state_cell(event_badge(e.state), event_tone(e.state), p.width),
        EventCol::Subject => text(&subject(e), p.width, Style::new().bold()),
        EventCol::Message => text(&e.message, p.width, Style::new()),
        EventCol::Ack => text(
            match (e.acked(), narrow) {
                (true, true) => "✔",
                (true, false) => "✔ acked",
                (false, _) => "",
            },
            p.width,
            dim(),
        ),
    }))
}

// ---- overview ----------------------------------------------------------

/// `● 0 online   ▲ 1 degraded` or, when that does not fit, `● 0  ▲ 1`.
fn counts_line(width: usize, items: &[(&str, &str, u64, Tone)]) -> Line<'static> {
    let full: usize = items
        .iter()
        .map(|(s, l, v, _)| format!("{s} {v} {l}   ").width())
        .sum();
    let compact = full > width;
    Line::from(
        items
            .iter()
            .map(|(symbol, label, value, tone)| {
                let tone = if *value == 0 { Tone::Dim } else { *tone };
                let text = if compact {
                    format!("{symbol} {value}  ")
                } else {
                    format!("{symbol} {value} {label}   ")
                };
                Span::styled(text, style(tone))
            })
            .collect::<Vec<_>>(),
    )
}

fn host_counts(o: &Overview) -> [(&'static str, &'static str, u64, Tone); 3] {
    [
        (
            "●",
            "online",
            o.hosts
                .total
                .saturating_sub(o.hosts.degraded + o.hosts.offline),
            Tone::Good,
        ),
        ("▲", "degraded", o.hosts.degraded, Tone::Warn),
        ("✘", "offline", o.hosts.offline, Tone::Bad),
    ]
}

fn service_counts(o: &Overview) -> [(&'static str, &'static str, u64, Tone); 4] {
    [
        ("✔", "ok", o.services.ok, Tone::Good),
        ("✘", "failed", o.services.failed, Tone::Bad),
        ("…", "pending", o.services.pending, Tone::Warn),
        ("○", "unmonitored", o.services.unmonitored, Tone::Dim),
    ]
}

fn render_overview(f: &mut Frame, app: &mut App, area: Rect, size: Size) {
    let (attention_area, events_area) = if size.narrow() {
        narrow_overview_top(f, app, area)
    } else {
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
        let inner = usize::from(hosts_area.width.saturating_sub(2));
        let hosts = vec![
            counts_line(inner, &host_counts(o)),
            Line::from(Span::styled(format!("{} in total", o.hosts.total), dim())),
        ];
        f.render_widget(
            Paragraph::new(hosts).block(block(title("Hosts"))),
            hosts_area,
        );
        let services = vec![
            counts_line(inner, &service_counts(o)),
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
        let [attention, events] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(bottom);
        (attention, events)
    };
    render_attention(f, app, attention_area, size);
    render_recent_events(f, app, events_area, size);
}

/// Stacks the summary card and the activity chart on top of a narrow overview and
/// returns the areas left for the two lists, one above the other.
fn narrow_overview_top(f: &mut Frame, app: &mut App, area: Rect) -> (Rect, Rect) {
    let h = area.height;
    let fleet_h = if h >= 16 { 4 } else { 0 };
    let chart_h = if h >= 26 { (h / 5).clamp(6, 10) } else { 0 };
    // the attention list is as tall as its entries (within reason); events get the rest
    let rest = h.saturating_sub(fleet_h + chart_h);
    let attention_h = (app.attention().len() as u16 + 2)
        .clamp(4, (rest * 3 / 5).max(4))
        .min(rest);
    let [fleet, chart, attention, events] = Layout::vertical([
        Constraint::Length(fleet_h),
        Constraint::Length(chart_h),
        Constraint::Length(attention_h),
        Constraint::Fill(1),
    ])
    .areas(area);
    let o = &app.overview;
    if fleet_h > 0 {
        let inner = usize::from(fleet.width.saturating_sub(2));
        let lines = vec![
            counts_line(inner, &host_counts(o)),
            counts_line(inner, &service_counts(o)),
        ];
        f.render_widget(
            Paragraph::new(lines).block(block(title("Hosts · Services"))),
            fleet,
        );
    }
    if chart_h > 0 {
        render_activity(f, o, chart);
    }
    (attention, events)
}

#[derive(Debug, Clone, Copy)]
enum AttentionCol {
    Badge,
    Subject,
    Detail,
}

fn render_attention(f: &mut Frame, app: &mut App, area: Rect, size: Size) {
    let narrow = size.narrow();
    let items = app.attention();
    let subject_of = |a: &super::app::Attention| match &a.service {
        Some(service) => format!("{}/{service}", a.host),
        None => a.host.clone(),
    };
    let longest = items
        .iter()
        .map(|a| subject_of(a).width())
        .max()
        .unwrap_or(0);
    let planned = layout::plan(
        &[
            Col {
                id: AttentionCol::Badge,
                header: "",
                width: Width::Fixed(if narrow { 2 } else { 14 }),
                priority: 9,
            },
            Col {
                id: AttentionCol::Subject,
                header: "",
                width: Width::content(10, longest, 40),
                priority: 10,
            },
            Col {
                id: AttentionCol::Detail,
                header: "",
                width: Width::flex(8, 1),
                priority: 5,
            },
        ],
        avail(area),
    );
    let rows: Vec<Row> = items
        .iter()
        .map(|a| {
            let (badge, tone) = match a.kind {
                AttentionKind::Offline => ("✘ offline", Tone::Bad),
                AttentionKind::Degraded => ("▲ degraded", Tone::Warn),
                AttentionKind::Failed => ("✘ failed", Tone::Bad),
                AttentionKind::Unmonitored => ("○ unmonitored", Tone::Dim),
            };
            let subject = subject_of(a);
            let detail = match a.kind {
                AttentionKind::Offline => format!("last seen {}", fmt::ago(a.since)),
                AttentionKind::Failed => format!("{}, {}", a.detail, fmt::ago(a.since)),
                _ => a.detail.clone(),
            };
            Row::new(planned.iter().map(|p| match p.id {
                AttentionCol::Badge => state_cell(badge, tone, p.width),
                AttentionCol::Subject => text(&subject, p.width, Style::new().bold()),
                AttentionCol::Detail => text(&detail, p.width, dim()),
            }))
        })
        .collect();
    let attention_title = if items.is_empty() {
        "Needs attention".to_owned()
    } else {
        format!("Needs attention · {}", items.len())
    };
    let focus = app.overview_focus == ListId::Attention;
    let empty = items.is_empty();
    render_list(
        f,
        &mut app.hits,
        ListId::Attention,
        area,
        block_focus(title(&attention_title), focus),
        None,
        rows,
        &widths_of(&planned),
        &mut app.attention_cursor,
        focus,
    );
    if empty {
        let text = if app.loaded {
            "✔ All systems operational"
        } else {
            "loading…"
        };
        let tone = if app.loaded { Tone::Good } else { Tone::Dim };
        let inner = block("").inner(area);
        f.render_widget(
            Paragraph::new(Span::styled(format!(" {text}"), style(tone))),
            inner,
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum RecentCol {
    Age,
    Glyph,
    Subject,
    Message,
}

fn render_recent_events(f: &mut Frame, app: &mut App, area: Rect, _size: Size) {
    let longest = longest_subject(&app.events);
    let planned = layout::plan(
        &[
            Col {
                id: RecentCol::Age,
                header: "",
                width: Width::Fixed(4),
                priority: 6,
            },
            Col {
                id: RecentCol::Glyph,
                header: "",
                width: Width::Fixed(1),
                priority: 9,
            },
            Col {
                id: RecentCol::Subject,
                header: "",
                width: Width::content(10, longest, 32),
                priority: 7,
            },
            Col {
                id: RecentCol::Message,
                header: "",
                width: Width::flex(8, 2),
                priority: 10,
            },
        ],
        avail(area),
    );
    let rows: Vec<Row> = app
        .events
        .iter()
        .map(|e| {
            Row::new(planned.iter().map(|p| match p.id {
                RecentCol::Age => text(&short_duration(fmt::now() - e.created_at), p.width, dim()),
                RecentCol::Glyph => text(
                    symbol(event_badge(e.state)),
                    p.width,
                    style(event_tone(e.state)),
                ),
                RecentCol::Subject => text(&subject(e), p.width, Style::new().bold()),
                RecentCol::Message => text(&e.message, p.width, dim()),
            }))
        })
        .collect();
    let events_title = format!("Recent events · {}", rows.len());
    let empty = rows.is_empty();
    let focus = app.overview_focus == ListId::OverviewEvents;
    render_list(
        f,
        &mut app.hits,
        ListId::OverviewEvents,
        area,
        block_focus(title(&events_title), focus),
        None,
        rows,
        &widths_of(&planned),
        &mut app.overview_events_cursor,
        focus,
    );
    if empty {
        let inner = block("").inner(area);
        f.render_widget(Paragraph::new(Span::styled(" no events yet", dim())), inner);
    }
}

/// Events of the last 24 hours as two stacked bar charts: succeeded above, failed below.
fn render_activity(f: &mut Frame, o: &Overview, area: Rect) {
    let failed: u64 = o.activity.iter().map(|b| b.failed).sum();
    let heading = if area.width < 50 {
        format!("Events 24h · {} · {failed}✘", o.events_24h)
    } else {
        format!("Events, 24h · {} events · {failed} failed", o.events_24h)
    };
    let frame = block(title(&heading));
    let inner = frame.inner(area);
    f.render_widget(frame, area);
    if inner.height < 3 || inner.width < 16 || o.activity.is_empty() {
        return;
    }
    let [plot, axis] = Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).areas(inner);
    let [ok_area, failed_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(plot);

    let compact = inner.width < 44;
    let gutter: u16 = if compact { 6 } else { 8 };
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
            if compact { "fail" } else { "failed" },
            o.activity.iter().map(|b| b.failed).collect(),
            Tone::Bad,
        ),
    ] {
        let [labels, chart] =
            Layout::horizontal([Constraint::Length(gutter), Constraint::Min(1)]).areas(area);
        chart_width = chart.width;
        let max = values.iter().copied().max().unwrap_or(0);
        let mut lines = vec![Line::from(Span::styled(
            format!(" {label}"),
            style(tone).bold(),
        ))];
        lines.push(Line::from(Span::styled(format!(" ≤{max}"), dim())));
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
    let middle = usize::from(chart_width).saturating_sub(7);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostCol {
    State,
    Name,
    Cpu,
    Mem,
    Load,
    Services,
    Seen,
    Uptime,
    Failing,
}

/// The widest values of a host table, so no column is wider than its content.
#[derive(Debug, Default, Clone, Copy)]
struct HostMeasure {
    name: usize,
    load: usize,
    services: usize,
}

fn measure_hosts(hosts: &[&HostSummary], narrow: bool) -> HostMeasure {
    let muted = if narrow { " ⊘" } else { " (muted)" };
    let mut m = HostMeasure::default();
    for h in hosts {
        let name = h.label().width()
            + if h.muted_until.is_some() {
                muted.width()
            } else {
                0
            };
        m.name = m.name.max(name);
        m.load = m.load.max(fmt::load(h.system.load).width());
        m.services = m.services.max(services_text(h).width());
    }
    m
}

fn services_text(h: &HostSummary) -> String {
    let plain = format!("{}/{}", h.services.ok, h.services.total);
    if h.services.failed > 0 {
        format!("{plain} ✘{}", h.services.failed)
    } else {
        plain
    }
}

fn host_cols(narrow: bool, m: HostMeasure) -> Vec<Col<HostCol>> {
    let sparkline = Width::Flex {
        min: 16,
        weight: 2,
        max: 46,
    };
    vec![
        Col {
            id: HostCol::State,
            header: if narrow { "" } else { "STATE" },
            width: Width::Fixed(if narrow { 2 } else { 12 }),
            priority: 9,
        },
        Col {
            id: HostCol::Name,
            header: "HOST",
            width: Width::content(6, m.name, 40),
            priority: 10,
        },
        Col {
            id: HostCol::Cpu,
            header: "CPU",
            width: sparkline,
            priority: 5,
        },
        Col {
            id: HostCol::Mem,
            header: "MEM",
            width: sparkline,
            priority: 4,
        },
        Col {
            id: HostCol::Load,
            header: "LOAD",
            // exactly the longest value: never truncated, dropped as a whole when short of room
            width: Width::Fixed(m.load.max(4) as u16),
            priority: 3,
        },
        Col {
            id: HostCol::Services,
            header: "SERVICES",
            width: Width::Fixed(m.services.max(8) as u16),
            priority: 8,
        },
        Col {
            id: HostCol::Seen,
            header: "SEEN",
            width: Width::Fixed(6),
            priority: 6,
        },
        Col {
            id: HostCol::Uptime,
            header: "UP",
            width: Width::Fixed(6),
            priority: 2,
        },
        Col {
            id: HostCol::Failing,
            header: "FAILING",
            width: Width::Flex {
                min: 14,
                weight: 1,
                max: 48,
            },
            priority: 1,
        },
    ]
}

/// `12%  ▁▂▃▅▇`: the current value, then a sparkline that fills the rest of the cell.
fn spark_cell(values: &[f64], percent: Option<f64>, color: Color, width: u16) -> Cell<'static> {
    let bar = usize::from(width).saturating_sub(6).max(4);
    Cell::from(Line::from(vec![
        Span::raw(format!("{:<5}", fmt::percent(percent))),
        Span::styled(fmt::sparkline(values, bar), Style::new().fg(color)),
    ]))
}

fn host_row(planned: &[Planned<HostCol>], h: &HostSummary, narrow: bool) -> Row<'static> {
    Row::new(planned.iter().map(|p| {
        match p.id {
            HostCol::State => state_cell(host_badge(h.state), host_tone(h.state), p.width),
            HostCol::Name => {
                // a muted host keeps its marker: an icon when space is short
                let suffix = if narrow { " ⊘" } else { " (muted)" };
                let width = usize::from(p.width);
                if h.muted_until.is_some() && width > suffix.width() + 3 {
                    let name = ellipsize(h.label(), width - suffix.width());
                    Cell::from(Line::from(vec![
                        Span::raw(name),
                        Span::styled(suffix, dim()),
                    ]))
                } else {
                    text(h.label(), p.width, Style::new())
                }
            }
            HostCol::Cpu => spark_cell(&h.sparkline.cpu, h.system.cpu, ACCENT, p.width),
            HostCol::Mem => spark_cell(&h.sparkline.mem, h.system.mem_percent, MEM, p.width),
            HostCol::Load => text(&fmt::load(h.system.load), p.width, Style::new()),
            HostCol::Services => {
                let plain = format!("{}/{}", h.services.ok, h.services.total);
                if h.services.failed > 0 {
                    let failed = format!(" ✘{}", h.services.failed);
                    if (plain.width() + failed.width()) as u16 <= p.width {
                        Cell::from(Line::from(vec![
                            Span::raw(plain),
                            Span::styled(failed, style(Tone::Bad)),
                        ]))
                    } else {
                        text(&format!("{plain}{failed}"), p.width, style(Tone::Bad))
                    }
                } else {
                    text(&plain, p.width, Style::new())
                }
            }
            HostCol::Seen => text(&short_duration(fmt::now() - h.last_seen), p.width, dim()),
            HostCol::Uptime => text(
                &h.system
                    .uptime
                    .map_or_else(|| "-".to_owned(), short_duration),
                p.width,
                dim(),
            ),
            HostCol::Failing => text(&h.failing.join(", "), p.width, style(Tone::Bad)),
        }
    }))
}

fn render_hosts(f: &mut Frame, app: &mut App, area: Rect, size: Size) {
    let extra = extras(app, Some(app.host_filter.label()));
    let narrow = size.narrow();
    let (planned, rows) = {
        let hosts = app.visible_hosts();
        let planned = layout::plan(
            &host_cols(narrow, measure_hosts(&hosts, narrow)),
            avail(area),
        );
        let rows: Vec<Row> = hosts
            .iter()
            .map(|h| host_row(&planned, h, narrow))
            .collect();
        (planned, rows)
    };
    let (shown, total) = (rows.len(), app.hosts.len());
    render_list(
        f,
        &mut app.hits,
        ListId::Hosts,
        area,
        table_block("Hosts", shown, total, &extra),
        Some(header_of(&planned)),
        rows,
        &widths_of(&planned),
        &mut app.hosts_cursor,
        true,
    );
    if shown == 0 {
        empty_hint(f, below_header(area), app.loaded, "hosts");
    }
}

fn render_services(f: &mut Frame, app: &mut App, area: Rect, size: Size) {
    let extra = extras(app, Some(app.service_filter.label()));
    let (planned, rows) = {
        let items = app.visible_services();
        let host_longest = items.iter().map(|s| s.host.width()).max().unwrap_or(0);
        let name_longest = items.iter().map(|s| s.name.width()).max().unwrap_or(0);
        let planned = layout::plan(
            &service_cols(size.narrow(), Some(host_longest), name_longest),
            avail(area),
        );
        let rows: Vec<Row> = items
            .iter()
            .map(|s| {
                service_row(
                    &planned,
                    &ServiceView {
                        state: s.state,
                        stale: s.stale,
                        host: &s.host,
                        name: &s.name,
                        kind: &s.kind,
                        status: service_status(&s.status_text, s.pending_action.as_deref()),
                        since: s.state_since,
                    },
                )
            })
            .collect();
        (planned, rows)
    };
    let (shown, total) = (rows.len(), app.services.len());
    render_list(
        f,
        &mut app.hits,
        ListId::Services,
        area,
        table_block("Services", shown, total, &extra),
        Some(header_of(&planned)),
        rows,
        &widths_of(&planned),
        &mut app.services_cursor,
        true,
    );
    if shown == 0 {
        empty_hint(f, below_header(area), app.loaded, "services");
    }
}

fn render_events(f: &mut Frame, app: &mut App, area: Rect, size: Size) {
    let mut extra = extras(app, None);
    if app.unacked_only {
        extra.insert(0, "unacked only".to_owned());
    }
    let narrow = size.narrow();
    let (planned, rows) = {
        let events = app.visible_events();
        let planned = layout::plan(
            &event_cols(narrow, longest_subject(events.iter().copied())),
            avail(area),
        );
        let rows: Vec<Row> = events
            .iter()
            .map(|e| event_row(&planned, e, narrow))
            .collect();
        (planned, rows)
    };
    let (shown, total) = (rows.len(), app.events.len());
    render_list(
        f,
        &mut app.hits,
        ListId::Events,
        area,
        table_block("Events", shown, total, &extra),
        Some(header_of(&planned)),
        rows,
        &widths_of(&planned),
        &mut app.events_cursor,
        true,
    );
    if shown == 0 {
        empty_hint(f, below_header(area), app.loaded, "events");
    }
}

/// How a muted host is described.
pub fn describe_mute(muted: &MutedUntil) -> String {
    match muted {
        MutedUntil::Forever(_) => "until unmuted".to_owned(),
        MutedUntil::Until(ts) => format!("until {}", fmt::clock(*ts)),
    }
}

// ---- popups --------------------------------------------------------------

pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
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
    let max_width = area.width.saturating_sub(2).max(10);
    let width = (prompt.chars().count() as u16 + 6)
        .clamp(30, 60)
        .min(max_width);
    let wrapped = layout::wrap(&prompt, usize::from(width.saturating_sub(4)));
    let height = wrapped.len() as u16 + 5;
    let rect = centered(area, width, height);
    f.render_widget(Clear, rect);
    let mut lines = vec![Line::from("")];
    lines.extend(
        wrapped
            .into_iter()
            .map(|l| Line::from(Span::styled(l, Style::new().bold())).alignment(Alignment::Center)),
    );
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled("y", Style::new().fg(Color::Green).bold()),
            Span::styled(" confirm   ", dim()),
            Span::styled("n", Style::new().fg(Color::Red).bold()),
            Span::styled(" cancel", dim()),
        ])
        .alignment(Alignment::Center),
    );
    let popup = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style(Tone::Warn))
        .title(Span::styled(" Confirm ", style(Tone::Warn).bold()));
    f.render_widget(Paragraph::new(lines).block(popup), rect);
}

const HELP: &[(&str, &str)] = &[
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
    ("c", "show the next chart on small screens"),
    ("^r, F5", "refresh now"),
    (
        "mouse",
        "click tabs and rows, double-click to open, wheel scrolls, right-click goes back",
    ),
    ("?", "this help; j/k scroll it"),
    ("^c", "quit"),
];

/// The help text laid out for `width` cells: key and description side by side,
/// or stacked when the screen is narrow.
fn help_lines(width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (key, what) in HELP {
        if width >= 50 {
            let rows = layout::wrap(what, width - 16);
            for (i, row) in rows.into_iter().enumerate() {
                let key_text = if i == 0 {
                    format!("  {key:<14}")
                } else {
                    " ".repeat(16)
                };
                lines.push(Line::from(vec![
                    Span::styled(key_text, Style::new().fg(ACCENT).bold()),
                    Span::raw(row),
                ]));
            }
        } else {
            lines.push(Line::from(Span::styled(
                format!(" {key}"),
                Style::new().fg(ACCENT).bold(),
            )));
            for row in layout::wrap(what, width.saturating_sub(3)) {
                lines.push(Line::from(format!("   {row}")));
            }
        }
    }
    lines
}

fn render_help(f: &mut Frame, app: &mut App, area: Rect) {
    let width = area.width.saturating_sub(2).clamp(20, 72);
    let lines = help_lines(usize::from(width.saturating_sub(2)));
    let height = (lines.len() as u16 + 2).min(area.height).max(3);
    let rect = centered(area, width, height);
    let visible = usize::from(height.saturating_sub(2));
    let max_scroll = lines.len().saturating_sub(visible);
    app.help_scroll = app.help_scroll.min(max_scroll);
    let heading = if max_scroll > 0 {
        "Keys · j/k scroll"
    } else {
        "Keys"
    };
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(lines)
            .scroll((app.help_scroll as u16, 0))
            .block(block(title(heading)).border_style(Style::new().fg(ACCENT))),
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
    use crate::tui::app::tests::{app, host, key};
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

    /// The sizes the layouts must handle: a tiny terminal, a phone in portrait,
    /// the same phone landscape, an 80x24 terminal and a wide one.
    const SIZES: [(u16, u16); 9] = [
        (10, 4),
        (32, 20),
        (40, 60),
        (45, 70),
        (60, 40),
        (80, 24),
        (100, 30),
        (130, 36),
        (200, 60),
    ];

    #[tokio::test]
    async fn every_view_renders_at_every_size_without_panicking() {
        let (mut app, _rx) = app();
        for tab in Tab::ALL {
            app.tab = tab;
            for (w, h) in SIZES {
                let screen = draw(&mut app, w, h);
                assert!(screen.lines().count() <= usize::from(h));
                assert!(
                    screen.lines().all(|l| l.width() <= usize::from(w)),
                    "{tab:?} at {w}x{h} overflows"
                );
            }
        }
        // pages and popups too
        feed_host_page(&mut app);
        for (w, h) in SIZES {
            draw(&mut app, w, h);
        }
        app.mode = Mode::Help;
        for (w, h) in SIZES {
            draw(&mut app, w, h);
        }
        app.mode = Mode::Confirm(Pending::Ack(vec![1, 2, 3]));
        for (w, h) in SIZES {
            draw(&mut app, w, h);
        }
    }

    fn feed_host_page(app: &mut App) {
        let mut data = HostDetail {
            summary: host(1, "web-1", HostState::Ok),
            ..Default::default()
        };
        data.services = vec![crate::types::Service {
            name: "nginx-with-a-very-long-service-name".into(),
            kind: "process".into(),
            ..Default::default()
        }];
        app.pages
            .push(Page::Host(Box::new(HostPage::for_test(1, "web-1", data))));
    }

    #[test]
    fn wide_header_is_one_line_with_brand_tabs_counts_and_state() {
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
    }

    #[test]
    fn narrow_header_drops_the_url_and_abbreviates_the_tabs() {
        let (mut app, _rx) = app();
        let screen = draw(&mut app, 45, 40);
        let first = screen.lines().next().unwrap();
        assert!(!first.contains("127.0.0.1"), "{first}");
        assert!(first.contains("1:Overview"), "{first}");
        assert!(
            first.contains(" 2 ") && first.contains(" 3 ") && first.contains(" 4 "),
            "{first}"
        );
        assert!(
            first.contains("●1") && first.contains("▲1") && first.contains("✘1"),
            "{first}"
        );
        app.tab = Tab::Hosts;
        let first = draw(&mut app, 45, 40).lines().next().unwrap().to_owned();
        assert!(first.contains("2:Hosts"), "{first}");
        // at 28 columns only the live dot is kept next to the tabs
        let first = draw(&mut app, 28, 20).lines().next().unwrap().to_owned();
        assert!(
            first.contains("2:Hosts") && !first.contains("✘1"),
            "{first}"
        );
    }

    #[test]
    fn hosts_view_adapts_its_columns() {
        let (mut app, _rx) = app();
        app.tab = Tab::Hosts;
        let wide = draw(&mut app, 130, 20);
        for needle in [
            "✘ offline",
            "▲ degraded",
            "web-1",
            "CPU",
            "MEM",
            "LOAD",
            "SERVICES",
            "SEEN",
        ] {
            assert!(wide.contains(needle), "wide lacks {needle}:\n{wide}");
        }
        let narrow = draw(&mut app, 45, 20);
        assert!(
            narrow.contains("HOST") && narrow.contains("SERVICES"),
            "{narrow}"
        );
        for gone in ["MEM", "LOAD"] {
            assert!(!narrow.contains(gone), "narrow still has {gone}:\n{narrow}");
        }
        assert!(narrow.contains('✘') && narrow.contains("db-1"), "{narrow}");
    }

    /// The blank cells right after `needle` on the first row that contains it.
    fn gap_after(screen: &str, needle: &str) -> usize {
        let row = screen
            .lines()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle}:\n{screen}"));
        let after = &row[row.find(needle).unwrap() + needle.len()..];
        after.chars().take_while(|c| *c == ' ').count()
    }

    #[test]
    fn wide_tables_are_sized_to_their_content() {
        let (mut app, _rx) = app();
        app.hosts[0].system.load = Some([16.56, 16.5, 16.62]);
        app.tab = Tab::Hosts;
        let screen = draw(&mut app, 150, 20);
        // the extra width goes to the sparklines and the extra columns, not to a gap
        for header in ["CPU", "MEM", "LOAD", "SERVICES", "SEEN", "UP", "FAILING"] {
            assert!(
                screen.contains(header),
                "no {header} column at 150 columns:\n{screen}"
            );
        }
        for name in ["db-1", "cache", "web-1"] {
            let gap = gap_after(&screen, name);
            assert!(gap <= 6, "{gap} blank cells after {name}:\n{screen}");
        }
        assert!(
            screen.contains("16.56 16.50 16.62"),
            "load is cut:\n{screen}"
        );
        assert!(screen.contains("0.40 0.30 0.20"), "{screen}");
        // the sparkline bars grew beyond the old fixed ten cells
        let cpu_row = screen.lines().find(|l| l.contains("web-1")).unwrap();
        assert!(
            cpu_row.chars().filter(|c| "▁▂▃▄▅▆▇█".contains(*c)).count() >= 5,
            "{cpu_row}"
        );

        // load is never truncated, at any width: a column that does not fit is dropped whole
        for width in [150, 130, 120, 100, 80, 60, 45, 32] {
            let screen = draw(&mut app, width, 20);
            if screen.contains("LOAD") {
                assert!(
                    screen.contains("16.56 16.50 16.62"),
                    "load cut at {width}:\n{screen}"
                );
            }
        }

        // the other tables have no dead space after short names either
        app.tab = Tab::Services;
        let services = draw(&mut app, 150, 20);
        assert!(gap_after(&services, "web-1") <= 6, "{services}");
        assert!(gap_after(&services, "nginx") <= 6, "{services}");
        app.tab = Tab::Events;
        let events = draw(&mut app, 150, 20);
        assert!(gap_after(&events, "web-1/sshd") <= 6, "{events}");
    }

    #[test]
    fn muted_hosts_keep_a_marker() {
        let (mut app, _rx) = app();
        app.hosts[0].muted_until = Some(crate::types::MutedUntil::Forever("forever".into()));
        app.tab = Tab::Hosts;
        assert!(draw(&mut app, 130, 20).contains("(muted)"));
        let narrow = draw(&mut app, 45, 20);
        assert!(
            narrow.contains('⊘') && !narrow.contains("(muted)"),
            "{narrow}"
        );
    }

    #[test]
    fn long_names_are_cut_with_an_ellipsis_not_wrapped() {
        let (mut app, _rx) = app();
        app.hosts[0].display_name = Some("a-really-long-host-name-that-never-fits".into());
        app.tab = Tab::Hosts;
        let screen = draw(&mut app, 40, 20);
        assert!(screen.contains('…'), "{screen}");
        assert!(!screen.contains("never-fits"), "{screen}");
    }

    #[test]
    fn overview_stacks_on_narrow_screens() {
        let (mut app, _rx) = app();
        let narrow = draw(&mut app, 45, 40);
        let row_of = |needle: &str| narrow.lines().position(|l| l.contains(needle));
        let (attention, events) = (row_of("Needs attention"), row_of("Recent events"));
        assert!(attention.is_some() && events.is_some(), "{narrow}");
        assert!(
            attention < events,
            "the lists are stacked, attention first:\n{narrow}"
        );
        assert!(row_of("Hosts · Services").is_some(), "{narrow}");
        assert!(row_of("24h").is_some(), "the chart stays:\n{narrow}");
        // side by side on wide screens: both titles on one row
        let wide = draw(&mut app, 130, 36);
        let line = wide
            .lines()
            .find(|l| l.contains("Needs attention"))
            .unwrap();
        assert!(line.contains("Recent events"), "{wide}");
    }

    #[test]
    fn narrow_footer_is_short() {
        let (mut app, _rx) = app();
        let narrow = draw(&mut app, 45, 30);
        let footer = narrow.lines().last().unwrap();
        assert!(
            footer.contains("? help") && footer.contains("q quit"),
            "{footer}"
        );
        assert!(!footer.contains("refresh"), "{footer}");
        let wide = draw(&mut app, 130, 30);
        assert!(wide.lines().last().unwrap().contains("refresh"));
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
    fn popups_fit_small_screens() {
        let (mut app, _rx) = app();
        app.tab = Tab::Services;
        app.mode = Mode::Confirm(Pending::Service {
            host_id: 1,
            host: "a-host-with-a-fairly-long-name".into(),
            service: "a-service-with-a-long-name".into(),
            action: crate::types::Action::Restart,
        });
        let screen = draw(&mut app, 32, 20);
        assert!(
            screen.contains("Confirm") && screen.contains("confirm"),
            "{screen}"
        );
        assert!(screen.lines().all(|l| l.width() <= 32));
        app.mode = Mode::Confirm(Pending::Ack(vec![1, 2]));
        assert!(draw(&mut app, 100, 24).contains("Acknowledge 2 event(s)?"));

        app.mode = Mode::Help;
        let wide = draw(&mut app, 100, 30);
        assert!(
            wide.contains("Keys") && wide.contains("right-click"),
            "{wide}"
        );
        let small = draw(&mut app, 40, 14);
        assert!(
            small.contains("j/k scroll"),
            "a short screen makes the help scrollable:\n{small}"
        );
    }

    #[test]
    fn help_scrolls_on_small_screens() {
        let (mut app, _rx) = app();
        app.mode = Mode::Help;
        let first = draw(&mut app, 40, 14);
        assert!(first.contains("Tab, 1-4"), "{first}");
        for _ in 0..40 {
            app.update(key(ratatui::crossterm::event::KeyCode::Down));
            assert_eq!(app.mode, Mode::Help, "scrolling keeps the help open");
        }
        let last = draw(&mut app, 40, 14);
        assert!(
            !last.contains("Tab, 1-4") && last.contains("quit"),
            "{last}"
        );
        app.update(key(ratatui::crossterm::event::KeyCode::Char('x')));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[tokio::test]
    async fn host_page_lists_services_and_draws_charts() {
        let (mut app, _rx) = app();
        feed_host_page(&mut app);
        let screen = draw(&mut app, 120, 40);
        assert!(screen.contains("nginx"), "{screen}");
        assert!(screen.contains("h/Esc back"), "{screen}");
        let narrow = draw(&mut app, 45, 70);
        assert!(narrow.contains("nginx") && narrow.contains('…'), "{narrow}");
        assert!(
            narrow.lines().last().unwrap().contains("h back"),
            "{narrow}"
        );
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
    async fn taps_land_on_the_right_rows_and_tabs_on_a_narrow_layout() {
        let (mut app, _rx) = app();
        draw(&mut app, 45, 40);
        // the abbreviated tab "3" selects the Services view
        let (_, rect) = *app
            .hits
            .tabs
            .iter()
            .find(|(t, _)| *t == Tab::Services)
            .unwrap();
        click(&mut app, rect.x, rect.y);
        assert_eq!(app.tab, Tab::Services);
        draw(&mut app, 45, 40);
        let hit = app
            .hits
            .lists
            .iter()
            .find(|h| h.id == ListId::Services)
            .copied()
            .unwrap();
        assert!(hit.outer.width <= 45);
        click(&mut app, hit.outer.x + 3, hit.outer.y + 2 + 2); // third row
        assert_eq!(app.services_cursor.selected(), 2);
        click(&mut app, hit.outer.x + 3, hit.outer.y + 2 + 2);
        assert!(matches!(app.pages.last(), Some(Page::Service(p)) if p.service == "redis"));
        // overview lists are stacked: both register full-width rectangles
        app.pages.clear();
        app.tab = Tab::Overview;
        draw(&mut app, 45, 40);
        let attention = app
            .hits
            .lists
            .iter()
            .find(|h| h.id == ListId::Attention)
            .copied()
            .unwrap();
        let events = app
            .hits
            .lists
            .iter()
            .find(|h| h.id == ListId::OverviewEvents)
            .copied()
            .unwrap();
        assert!(events.outer.y >= attention.outer.bottom());
        click(&mut app, events.outer.x + 2, events.outer.y + 1);
        assert_eq!(app.overview_focus, ListId::OverviewEvents);
    }

    #[tokio::test]
    async fn the_layout_follows_resizes() {
        let (mut app, _rx) = app();
        let wide = draw(&mut app, 130, 36);
        let narrow = draw(&mut app, 45, 60);
        let wide_again = draw(&mut app, 130, 36);
        assert_eq!(wide.lines().count(), wide_again.lines().count());
        assert_ne!(wide, narrow);
        // the hit rectangles are those of the last frame
        assert!(app.hits.tabs.iter().all(|(_, r)| r.right() <= 130));
        draw(&mut app, 45, 60);
        assert!(app.hits.tabs.iter().all(|(_, r)| r.right() <= 45));
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
}
