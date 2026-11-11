//! TUI state and its reaction to input, API results and live updates.
//!
//! Nothing here blocks: HTTP calls run in spawned tasks and report back with a
//! [`Msg`], so the UI thread only ever touches in-memory state.
//!
//! The four views (tabs) are the base. Drilling in (Enter, Right, `l`) pushes a
//! [`Page`] on a stack; going back (Left, `h`, Esc, `q`) pops it, which leaves the
//! view below exactly as it was: its tab, focused pane and selection.

use std::future::Future;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use ratatui::widgets::TableState;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use super::charts::{self, Charts, Range, Spec};
use crate::api::{ApiError, Client, decode};
use crate::fmt::Tone;
use crate::types::{
    Action, Event, EventsResponse, HostDetail, HostState, HostSummary, Me, MetricsResponse,
    Overview, Service, ServiceDetail, ServiceListItem, ServiceState, ServicesResponse,
    StreamMessage,
};

pub type Tx = UnboundedSender<Msg>;

/// Everything the main loop reacts to.
pub enum Msg {
    Input(TermEvent),
    Tick,
    Snapshot(Result<Box<Snapshot>, ApiError>),
    Host(u64, Result<Box<HostDetail>, ApiError>),
    Service(u64, String, Result<Box<ServiceDetail>, ApiError>),
    Metrics {
        host_id: u64,
        service: String,
        range: Range,
        now: f64,
        result: Result<Box<MetricsResponse>, ApiError>,
    },
    Stream(StreamMessage),
    Conn(Conn),
    Role(Option<String>),
    Done {
        label: String,
        acked: Vec<u64>,
        result: Result<(), ApiError>,
    },
}

/// The state of the live event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conn {
    Connecting,
    Live,
    Reconnecting,
}

/// Everything shown in the lists, fetched in one go.
pub struct Snapshot {
    pub overview: Overview,
    pub hosts: Vec<HostSummary>,
    pub services: Vec<ServiceListItem>,
    pub events: Vec<Event>,
}

pub async fn fetch_snapshot(client: &Client) -> Result<Snapshot, ApiError> {
    let (overview, hosts, services, events) = tokio::try_join!(
        client.overview(),
        client.hosts(),
        client.services(None, None),
        client.events(None, false, 200)
    )?;
    Ok(Snapshot {
        overview: decode(overview)?,
        hosts: decode(hosts)?,
        services: decode::<ServicesResponse>(services)?.services,
        events: decode::<EventsResponse>(events)?.events,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Hosts,
    Services,
    Events,
}

impl Tab {
    pub const ALL: [Self; 4] = [Self::Overview, Self::Hosts, Self::Services, Self::Events];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    fn step(self, delta: isize) -> Self {
        let n = Self::ALL.len() as isize;
        Self::ALL[(self.index() as isize + delta).rem_euclid(n) as usize]
    }
}

/// A selectable list on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListId {
    Attention,
    OverviewEvents,
    Hosts,
    Services,
    Events,
    HostServices,
    ServiceEvents,
}

/// Where the last frame put things, so mouse clicks can be mapped back.
#[derive(Debug, Clone, Copy)]
pub struct ListHit {
    pub id: ListId,
    /// The whole bordered widget.
    pub outer: Rect,
    /// Whether the first row inside the border is a header.
    pub header: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Hits {
    pub tabs: Vec<(Tab, Rect)>,
    pub lists: Vec<ListHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostFilter {
    #[default]
    All,
    Offline,
    Degraded,
    Ok,
}

impl HostFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Offline => "offline",
            Self::Degraded => "degraded",
            Self::Ok => "ok",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Offline,
            Self::Offline => Self::Degraded,
            Self::Degraded => Self::Ok,
            Self::Ok => Self::All,
        }
    }

    fn accepts(self, host: &HostSummary) -> bool {
        match self {
            Self::All => true,
            Self::Offline => host.state == HostState::Offline,
            Self::Degraded => host.state == HostState::Degraded,
            Self::Ok => host.state == HostState::Ok,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServiceFilter {
    #[default]
    All,
    Failed,
    Unmonitored,
    Pending,
    Ok,
    Stale,
}

impl ServiceFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Failed => "failed",
            Self::Unmonitored => "unmonitored",
            Self::Pending => "pending",
            Self::Ok => "ok",
            Self::Stale => "stale",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Failed,
            Self::Failed => Self::Unmonitored,
            Self::Unmonitored => Self::Pending,
            Self::Pending => Self::Ok,
            Self::Ok => Self::Stale,
            Self::Stale => Self::All,
        }
    }

    fn accepts(self, service: &ServiceListItem) -> bool {
        match self {
            Self::All => true,
            Self::Stale => service.stale,
            Self::Failed => !service.stale && service.state == ServiceState::Failed,
            Self::Unmonitored => !service.stale && service.state == ServiceState::Unmonitored,
            Self::Pending => {
                !service.stale
                    && matches!(service.state, ServiceState::Pending | ServiceState::Init)
            }
            Self::Ok => !service.stale && service.state == ServiceState::Ok,
        }
    }
}

/// Selection of a table, kept across frames so scrolling feels natural.
#[derive(Debug, Default)]
pub struct Cursor {
    pub state: TableState,
}

impl Cursor {
    pub fn selected(&self) -> usize {
        self.state.selected().unwrap_or(0)
    }

    pub fn set(&mut self, index: usize, len: usize) {
        self.state.select((len > 0).then(|| index.min(len - 1)));
    }

    /// Keeps the selection valid for a list of `len` rows, selecting the first one initially.
    pub fn sync(&mut self, len: usize) {
        self.set(self.selected(), len);
    }

    pub fn step(&mut self, delta: isize, len: usize) {
        let target = (self.selected() as isize + delta).clamp(0, len.max(1) as isize - 1);
        self.set(target as usize, len);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionKind {
    Offline,
    Degraded,
    Failed,
    Unmonitored,
}

/// One line of the overview's "Needs attention" list.
#[derive(Debug, Clone)]
pub struct Attention {
    pub kind: AttentionKind,
    pub host_id: u64,
    pub host: String,
    pub service: Option<String>,
    pub detail: String,
    pub since: Option<f64>,
}

pub struct HostPage {
    pub host_id: u64,
    pub name: String,
    pub data: Option<HostDetail>,
    pub cursor: Cursor,
    pub error: Option<String>,
    pub charts: Charts,
    pub range: Range,
    select_service: Option<String>,
}

pub struct ServicePage {
    pub host_id: u64,
    pub host: String,
    pub service: String,
    pub data: Option<ServiceDetail>,
    pub cursor: Cursor,
    pub error: Option<String>,
    pub charts: Charts,
    pub range: Range,
}

pub struct EventPage {
    pub event: Event,
}

/// A drill-down page on top of the current view.
pub enum Page {
    Host(Box<HostPage>),
    Service(Box<ServicePage>),
    Event(Box<EventPage>),
}

/// Which metrics a page charts, and for which service.
pub struct ChartTarget {
    pub host_id: u64,
    pub service: String,
    pub specs: &'static [Spec],
    pub range: Range,
}

impl Page {
    fn host_id(&self) -> Option<u64> {
        match self {
            Self::Host(p) => Some(p.host_id),
            Self::Service(p) => Some(p.host_id),
            Self::Event(p) => p.event.host_id,
        }
    }

    pub fn chart_target(&self) -> Option<ChartTarget> {
        match self {
            Self::Host(p) => {
                let system = p
                    .data
                    .as_ref()?
                    .services
                    .iter()
                    .find(|s| s.kind == "system")?;
                Some(ChartTarget {
                    host_id: p.host_id,
                    service: system.name.clone(),
                    specs: charts::specs_for("system"),
                    range: p.range,
                })
            }
            Self::Service(p) => {
                let kind = &p.data.as_ref()?.service.kind;
                Some(ChartTarget {
                    host_id: p.host_id,
                    service: p.service.clone(),
                    specs: charts::specs_for(kind),
                    range: p.range,
                })
            }
            Self::Event(_) => None,
        }
    }

    fn set_charts(&mut self, value: Charts) {
        match self {
            Self::Host(p) => p.charts = value,
            Self::Service(p) => p.charts = value,
            Self::Event(_) => {}
        }
    }
}

#[cfg(test)]
impl HostPage {
    pub fn for_test(host_id: u64, name: &str, data: HostDetail) -> Self {
        Self {
            host_id,
            name: name.to_owned(),
            data: Some(data),
            cursor: Cursor::default(),
            error: None,
            charts: Charts::Loading,
            range: Range::default(),
            select_service: None,
        }
    }
}

/// An action waiting for the user's confirmation.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    Service {
        host_id: u64,
        host: String,
        service: String,
        action: Action,
    },
    Ack(Vec<u64>),
}

impl Pending {
    pub fn prompt(&self) -> String {
        match self {
            Self::Service {
                host,
                service,
                action,
                ..
            } => {
                format!(
                    "{} service {service} on {host}?",
                    capitalize(action.as_str())
                )
            }
            Self::Ack(ids) => format!("Acknowledge {} event(s)?", ids.len()),
        }
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Search,
    Confirm(Pending),
    Help,
}

pub struct Toast {
    pub text: String,
    pub tone: Tone,
    until: Instant,
}

impl Toast {
    pub fn visible(&self) -> bool {
        Instant::now() < self.until
    }
}

/// The service a service action would apply to.
#[derive(Debug, Clone)]
struct ServiceTarget {
    host_id: u64,
    host: String,
    service: String,
    state: ServiceState,
}

const REFRESH_EVERY: Duration = Duration::from_secs(30);
const DIRTY_REFRESH_AFTER: Duration = Duration::from_secs(5);
const MAX_EVENTS: usize = 500;

pub struct App {
    client: Client,
    tx: Tx,
    pub base_url: String,

    pub tab: Tab,
    pub mode: Mode,
    pub quit: bool,

    pub overview: Overview,
    pub hosts: Vec<HostSummary>,
    pub services: Vec<ServiceListItem>,
    pub events: Vec<Event>,
    pub loaded: bool,

    pub conn: Conn,
    pub api_error: Option<String>,
    pub role: Option<String>,
    pub toast: Option<Toast>,

    pub hosts_cursor: Cursor,
    pub services_cursor: Cursor,
    pub events_cursor: Cursor,
    pub attention_cursor: Cursor,
    pub overview_events_cursor: Cursor,
    /// The focused list of the overview.
    pub overview_focus: ListId,
    /// Search text per tab, indexed by [`Tab::index`].
    pub search: [String; 4],
    pub host_filter: HostFilter,
    pub service_filter: ServiceFilter,
    pub unacked_only: bool,
    /// Drill-down pages on top of the current tab, the last one is shown.
    pub pages: Vec<Page>,
    /// Filled in by the renderer for mouse hit testing.
    pub hits: Hits,

    refreshing: bool,
    last_refresh: Instant,
    dirty: bool,
    refresh_at: Option<Instant>,
}

impl App {
    pub fn new(client: Client, tx: Tx) -> Self {
        Self {
            base_url: client
                .base_url()
                .to_string()
                .trim_end_matches('/')
                .to_owned(),
            client,
            tx,
            tab: Tab::Overview,
            mode: Mode::Normal,
            quit: false,
            overview: Overview::default(),
            hosts: Vec::new(),
            services: Vec::new(),
            events: Vec::new(),
            loaded: false,
            conn: Conn::Connecting,
            api_error: None,
            role: None,
            toast: None,
            hosts_cursor: Cursor::default(),
            services_cursor: Cursor::default(),
            events_cursor: Cursor::default(),
            attention_cursor: Cursor::default(),
            overview_events_cursor: Cursor::default(),
            overview_focus: ListId::Attention,
            search: Default::default(),
            host_filter: HostFilter::default(),
            service_filter: ServiceFilter::default(),
            unacked_only: false,
            pages: Vec::new(),
            hits: Hits::default(),
            refreshing: false,
            last_refresh: Instant::now(),
            dirty: false,
            refresh_at: None,
        }
    }

    /// Kicks off the first load.
    pub fn start(&mut self) {
        self.refresh();
        let (client, tx) = (self.client.clone(), self.tx.clone());
        tokio::spawn(async move {
            let role = match client.me().await.and_then(decode::<Me>) {
                Ok(me) => me.user.map(|u| u.role),
                Err(_) => None,
            };
            let _ = tx.send(Msg::Role(role));
        });
    }

    // ---- derived views -------------------------------------------------

    pub fn read_only(&self) -> bool {
        matches!(self.role.as_deref(), Some("viewer" | "collector"))
    }

    pub fn unacked_count(&self) -> usize {
        self.events.iter().filter(|e| !e.acked()).count()
    }

    pub fn search_text(&self) -> &str {
        &self.search[self.tab.index()]
    }

    pub fn visible_hosts(&self) -> Vec<&HostSummary> {
        let needle = self.search[Tab::Hosts.index()].to_lowercase();
        let rank = |h: &HostSummary| match h.state {
            HostState::Offline => 0,
            HostState::Degraded => 1,
            HostState::Ok => 2,
            HostState::Unknown => 3,
        };
        let mut hosts: Vec<&HostSummary> = self
            .hosts
            .iter()
            .filter(|h| self.host_filter.accepts(h))
            .filter(|h| {
                contains(h.label(), &needle)
                    || contains(&h.hostname, &needle)
                    || h.description
                        .as_deref()
                        .is_some_and(|d| contains(d, &needle))
            })
            .collect();
        hosts.sort_by_cached_key(|h| (rank(h), h.label().to_lowercase()));
        hosts
    }

    pub fn visible_services(&self) -> Vec<&ServiceListItem> {
        let needle = self.search[Tab::Services.index()].to_lowercase();
        self.services
            .iter()
            .filter(|s| self.service_filter.accepts(s))
            .filter(|s| {
                contains(&s.name, &needle)
                    || contains(&s.host, &needle)
                    || contains(&s.kind, &needle)
            })
            .collect()
    }

    pub fn visible_events(&self) -> Vec<&Event> {
        let needle = self.search[Tab::Events.index()].to_lowercase();
        self.events
            .iter()
            .filter(|e| !self.unacked_only || !e.acked())
            .filter(|e| {
                contains(&e.message, &needle)
                    || e.host.as_deref().is_some_and(|h| contains(h, &needle))
                    || e.service.as_deref().is_some_and(|s| contains(s, &needle))
            })
            .collect()
    }

    /// What needs attention: offline and degraded hosts, failing and unmonitored services.
    pub fn attention(&self) -> Vec<Attention> {
        let mut items = Vec::new();
        for h in self.hosts.iter().filter(|h| h.state == HostState::Offline) {
            items.push(Attention {
                kind: AttentionKind::Offline,
                host_id: h.id,
                host: h.label().to_owned(),
                service: None,
                detail: String::new(),
                since: Some(h.last_seen),
            });
        }
        for h in self.hosts.iter().filter(|h| h.state == HostState::Degraded) {
            items.push(Attention {
                kind: AttentionKind::Degraded,
                host_id: h.id,
                host: h.label().to_owned(),
                service: None,
                detail: format!("failing: {}", h.failing.join(", ")),
                since: None,
            });
        }
        let refs = |list: &[crate::types::ServiceRef], kind| {
            list.iter()
                .map(|s| Attention {
                    kind,
                    host_id: s.host_id,
                    host: s.host.clone(),
                    service: Some(s.service.clone()),
                    detail: s.status_text.clone(),
                    since: s.since,
                })
                .collect::<Vec<_>>()
        };
        items.extend(refs(&self.overview.failing, AttentionKind::Failed));
        items.extend(refs(&self.overview.unmonitored, AttentionKind::Unmonitored));
        items
    }

    /// The services of the open host page, worst first.
    pub fn detail_services(&self) -> Vec<&Service> {
        let Some(Page::Host(page)) = self.pages.last() else {
            return Vec::new();
        };
        let Some(data) = page.data.as_ref() else {
            return Vec::new();
        };
        sorted_services(data)
    }

    /// The newest copy of an event: the list may have it acknowledged since the page opened.
    pub fn event_now<'a>(&'a self, event: &'a Event) -> &'a Event {
        self.events
            .iter()
            .find(|e| e.id == event.id)
            .unwrap_or(event)
    }

    fn active_list(&self) -> Option<ListId> {
        match self.pages.last() {
            Some(Page::Host(_)) => Some(ListId::HostServices),
            Some(Page::Service(_)) => Some(ListId::ServiceEvents),
            Some(Page::Event(_)) => None,
            None => Some(match self.tab {
                Tab::Overview => self.overview_focus,
                Tab::Hosts => ListId::Hosts,
                Tab::Services => ListId::Services,
                Tab::Events => ListId::Events,
            }),
        }
    }

    fn list_len(&self, id: ListId) -> usize {
        match id {
            ListId::Attention => self.attention().len(),
            ListId::OverviewEvents => self.events.len(),
            ListId::Hosts => self.visible_hosts().len(),
            ListId::Services => self.visible_services().len(),
            ListId::Events => self.visible_events().len(),
            ListId::HostServices => self.detail_services().len(),
            ListId::ServiceEvents => match self.pages.last() {
                Some(Page::Service(p)) => p.data.as_ref().map_or(0, |d| d.recent_events.len()),
                _ => 0,
            },
        }
    }

    fn cursor_mut(&mut self, id: ListId) -> Option<&mut Cursor> {
        match id {
            ListId::Attention => Some(&mut self.attention_cursor),
            ListId::OverviewEvents => Some(&mut self.overview_events_cursor),
            ListId::Hosts => Some(&mut self.hosts_cursor),
            ListId::Services => Some(&mut self.services_cursor),
            ListId::Events => Some(&mut self.events_cursor),
            ListId::HostServices => match self.pages.last_mut() {
                Some(Page::Host(p)) => Some(&mut p.cursor),
                _ => None,
            },
            ListId::ServiceEvents => match self.pages.last_mut() {
                Some(Page::Service(p)) => Some(&mut p.cursor),
                _ => None,
            },
        }
    }

    fn selected_service(&self) -> Option<ServiceTarget> {
        match self.pages.last() {
            Some(Page::Host(page)) => {
                let services = self.detail_services();
                let service = services.get(page.cursor.selected())?;
                Some(ServiceTarget {
                    host_id: page.host_id,
                    host: page.name.clone(),
                    service: service.name.clone(),
                    state: service.state,
                })
            }
            Some(Page::Service(page)) => Some(ServiceTarget {
                host_id: page.host_id,
                host: page.host.clone(),
                service: page.service.clone(),
                state: page
                    .data
                    .as_ref()
                    .map_or(ServiceState::Unknown, |d| d.service.state),
            }),
            Some(Page::Event(_)) => None,
            None if self.tab == Tab::Services => {
                let services = self.visible_services();
                let s = services.get(self.services_cursor.selected())?;
                Some(ServiceTarget {
                    host_id: s.host_id,
                    host: s.host.clone(),
                    service: s.name.clone(),
                    state: s.state,
                })
            }
            None => None,
        }
    }

    // ---- messages ------------------------------------------------------

    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Input(TermEvent::Key(key)) => self.on_key(key),
            Msg::Input(TermEvent::Mouse(mouse)) => self.on_mouse(mouse),
            Msg::Input(_) => {}
            Msg::Tick => self.on_tick(),
            Msg::Snapshot(result) => self.on_snapshot(result),
            Msg::Host(id, result) => self.on_host(id, result),
            Msg::Service(id, name, result) => self.on_service(id, &name, result),
            Msg::Metrics {
                host_id,
                service,
                range,
                now,
                result,
            } => self.on_metrics(host_id, &service, range, now, result),
            Msg::Stream(message) => self.on_stream(message),
            Msg::Conn(conn) => self.conn = conn,
            Msg::Role(role) => self.role = role,
            Msg::Done {
                label,
                acked,
                result,
            } => self.on_done(&label, &acked, result),
        }
    }

    fn on_tick(&mut self) {
        if self.toast.as_ref().is_some_and(|t| !t.visible()) {
            self.toast = None;
        }
        let now = Instant::now();
        let due = self.refresh_at.is_some_and(|at| now >= at)
            || (self.dirty && now.duration_since(self.last_refresh) >= DIRTY_REFRESH_AFTER)
            || now.duration_since(self.last_refresh) >= REFRESH_EVERY;
        if due {
            self.refresh();
        }
    }

    pub fn refresh(&mut self) {
        self.refresh_at = None;
        if self.refreshing {
            self.dirty = true;
            return;
        }
        self.refreshing = true;
        self.dirty = false;
        self.last_refresh = Instant::now();
        let (client, tx) = (self.client.clone(), self.tx.clone());
        tokio::spawn(async move {
            let result = fetch_snapshot(&client).await.map(Box::new);
            let _ = tx.send(Msg::Snapshot(result));
        });
        self.reload_top();
    }

    /// Fetches the data of the page on top again (its charts follow when it arrives).
    fn reload_top(&self) {
        match self.pages.last() {
            Some(Page::Host(p)) => self.fetch_host(p.host_id),
            Some(Page::Service(p)) => self.fetch_service(p.host_id, &p.service),
            Some(Page::Event(_)) | None => {}
        }
    }

    fn fetch_host(&self, id: u64) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        tokio::spawn(async move {
            let result = client
                .host(id)
                .await
                .and_then(decode::<HostDetail>)
                .map(Box::new);
            let _ = tx.send(Msg::Host(id, result));
        });
    }

    fn fetch_service(&self, host_id: u64, service: &str) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        let service = service.to_owned();
        tokio::spawn(async move {
            let result = client
                .service(host_id, &service)
                .await
                .and_then(decode::<ServiceDetail>)
                .map(Box::new);
            let _ = tx.send(Msg::Service(host_id, service, result));
        });
    }

    /// Fetches the metric history for the page on top.
    fn fetch_charts(&self) {
        let Some(target) = self.pages.last().and_then(Page::chart_target) else {
            return;
        };
        if target.specs.is_empty() {
            return;
        }
        let (client, tx) = (self.client.clone(), self.tx.clone());
        let now = crate::fmt::now();
        tokio::spawn(async move {
            let keys: Vec<&str> = target.specs.iter().map(|s| s.key).collect();
            let result = client
                .metrics(
                    target.host_id,
                    &target.service,
                    &keys,
                    now - target.range.secs(),
                    now,
                )
                .await
                .and_then(decode::<MetricsResponse>)
                .map(Box::new);
            let _ = tx.send(Msg::Metrics {
                host_id: target.host_id,
                service: target.service,
                range: target.range,
                now,
                result,
            });
        });
    }

    fn on_snapshot(&mut self, result: Result<Box<Snapshot>, ApiError>) {
        self.refreshing = false;
        match result {
            Ok(snapshot) => {
                let snapshot = *snapshot;
                self.overview = snapshot.overview;
                self.hosts = snapshot.hosts;
                self.services = snapshot.services;
                self.events = snapshot.events;
                self.loaded = true;
                self.api_error = None;
            }
            Err(err) => {
                self.api_error = Some(err.to_string());
                self.set_toast(err.to_string(), Tone::Bad, 6);
            }
        }
    }

    fn on_host(&mut self, id: u64, result: Result<Box<HostDetail>, ApiError>) {
        let Some(index) = self
            .pages
            .iter()
            .position(|p| matches!(p, Page::Host(h) if h.host_id == id))
        else {
            return;
        };
        let is_top = index + 1 == self.pages.len();
        let Page::Host(page) = &mut self.pages[index] else {
            return;
        };
        match result {
            Ok(data) => {
                page.error = None;
                page.data = Some(*data);
            }
            Err(err) => page.error = Some(err.to_string()),
        }
        // Cursor positions refer to the sorted list.
        let names: Vec<&str> = page
            .data
            .as_ref()
            .map(|d| {
                sorted_services(d)
                    .into_iter()
                    .map(|s| s.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let selected = match page.select_service.take() {
            Some(wanted) => names.iter().position(|n| *n == wanted).unwrap_or(0),
            None => page.cursor.selected(),
        };
        let len = names.len();
        page.cursor.set(selected, len);
        if is_top && page.data.is_some() {
            self.fetch_charts();
        }
    }

    fn on_service(
        &mut self,
        host_id: u64,
        name: &str,
        result: Result<Box<ServiceDetail>, ApiError>,
    ) {
        let Some(index) = self.pages.iter().position(
            |p| matches!(p, Page::Service(s) if s.host_id == host_id && s.service == name),
        ) else {
            return;
        };
        let is_top = index + 1 == self.pages.len();
        let Page::Service(page) = &mut self.pages[index] else {
            return;
        };
        match result {
            Ok(data) => {
                page.error = None;
                let len = data.recent_events.len();
                page.data = Some(*data);
                let selected = page.cursor.selected();
                page.cursor.set(selected, len);
            }
            Err(err) => page.error = Some(err.to_string()),
        }
        if is_top && page.data.is_some() {
            self.fetch_charts();
        }
    }

    fn on_metrics(
        &mut self,
        host_id: u64,
        service: &str,
        range: Range,
        now: f64,
        result: Result<Box<MetricsResponse>, ApiError>,
    ) {
        let value = match result {
            Ok(response) => Charts::Ready(charts::parse(&response, now)),
            Err(err) => Charts::Failed(err.to_string()),
        };
        for page in &mut self.pages {
            let matches = page
                .chart_target()
                .is_some_and(|t| t.host_id == host_id && t.service == service && t.range == range);
            if matches {
                page.set_charts(value.clone());
            }
        }
    }

    fn on_stream(&mut self, message: StreamMessage) {
        match message {
            StreamMessage::Host { host } => {
                let host = *host;
                let changed = match self.hosts.iter_mut().find(|h| h.id == host.id) {
                    Some(known) => {
                        let changed = known.state != host.state
                            || known.services.failed != host.services.failed
                            || known.services.unmonitored != host.services.unmonitored
                            || known.services.total != host.services.total;
                        *known = host;
                        changed
                    }
                    None => {
                        self.hosts.push(host);
                        true
                    }
                };
                self.dirty |= changed;
            }
            StreamMessage::HostRemoved { id } => {
                self.hosts.retain(|h| h.id != id);
                self.services.retain(|s| s.host_id != id);
                if self.pages.iter().any(|p| p.host_id() == Some(id)) {
                    self.pages.retain(|p| p.host_id() != Some(id));
                    self.set_toast("The host was removed", Tone::Warn, 4);
                }
                self.dirty = true;
            }
            StreamMessage::Event { event } => {
                let event = *event;
                match self.events.iter_mut().find(|e| e.id == event.id) {
                    Some(known) => *known = event,
                    None => {
                        self.events.insert(0, event);
                        self.events.truncate(MAX_EVENTS);
                        for cursor in [&mut self.events_cursor, &mut self.overview_events_cursor] {
                            if cursor.selected() > 0 {
                                cursor.state.select_next();
                            }
                        }
                    }
                }
                self.dirty = true;
            }
            StreamMessage::Ping | StreamMessage::Other => {}
        }
    }

    fn on_done(&mut self, label: &str, acked: &[u64], result: Result<(), ApiError>) {
        match result {
            Ok(()) => {
                let now = crate::fmt::now();
                let mark = |event: &mut Event| {
                    if acked.contains(&event.id) {
                        event.acked_at = Some(now);
                        event.acked_by.get_or_insert_with(|| "you".to_owned());
                    }
                };
                self.events.iter_mut().for_each(mark);
                for page in &mut self.pages {
                    if let Page::Event(p) = page {
                        mark(&mut p.event);
                    }
                }
                self.set_toast(format!("✔ {label}"), Tone::Good, 4);
                // Monit applies actions asynchronously: look again shortly.
                self.refresh();
                self.refresh_at = Some(Instant::now() + Duration::from_secs(3));
            }
            Err(err) if err.is_forbidden() => {
                self.set_toast(
                    format!("✘ {label}: permission denied (this token is read-only)"),
                    Tone::Bad,
                    8,
                );
            }
            Err(err) => self.set_toast(format!("✘ {label}: {err}"), Tone::Bad, 8),
        }
    }

    pub fn set_toast(&mut self, text: impl Into<String>, tone: Tone, secs: u64) {
        self.toast = Some(Toast {
            text: text.into(),
            tone,
            until: Instant::now() + Duration::from_secs(secs),
        });
    }

    // ---- keys ----------------------------------------------------------

    fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => self.on_normal_key(key),
            Mode::Help => {}
            Mode::Search => self.on_search_key(key),
            Mode::Confirm(pending) => {
                if matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter) {
                    self.execute(pending);
                }
            }
        }
    }

    fn on_search_key(&mut self, key: KeyEvent) {
        let index = self.tab.index();
        self.mode = Mode::Search;
        match key.code {
            KeyCode::Enter => self.mode = Mode::Normal,
            KeyCode::Esc => {
                self.search[index].clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search[index].pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search[index].clear();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search[index].push(c);
            }
            _ => {}
        }
        // A new filter may leave the selection past the end.
        if let Some(id) = self.active_list() {
            let len = self.list_len(id);
            if let Some(cursor) = self.cursor_mut(id) {
                cursor.set(0, len);
            }
        }
    }

    fn on_normal_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let paged = !self.pages.is_empty();
        let overview = !paged && self.tab == Tab::Overview;
        let events_tab = !paged && self.tab == Tab::Events;
        match key.code {
            KeyCode::Char('q') if paged => self.back(),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc if paged => self.back(),
            KeyCode::Esc => {
                let index = self.tab.index();
                self.search[index].clear();
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace if paged => self.back(),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.drill(),
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Tab => self.switch_tab(self.tab.step(1)),
            KeyCode::BackTab => self.switch_tab(self.tab.step(-1)),
            KeyCode::Char(c @ '1'..='4') => {
                self.switch_tab(Tab::ALL[usize::from(c as u8 - b'1')]);
            }
            KeyCode::Char('p' | 'P') if overview => {
                self.overview_focus = match self.overview_focus {
                    ListId::Attention => ListId::OverviewEvents,
                    _ => ListId::Attention,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => self.nav(Nav::By(1)),
            KeyCode::Up | KeyCode::Char('k') => self.nav(Nav::By(-1)),
            KeyCode::PageDown => self.nav(Nav::By(10)),
            KeyCode::Char('d') if ctrl => self.nav(Nav::By(10)),
            KeyCode::PageUp => self.nav(Nav::By(-10)),
            KeyCode::Char('u') if ctrl => self.nav(Nav::By(-10)),
            KeyCode::Home | KeyCode::Char('g') => self.nav(Nav::First),
            KeyCode::End | KeyCode::Char('G') => self.nav(Nav::Last),
            KeyCode::Char('/') if !paged && self.tab != Tab::Overview => self.mode = Mode::Search,
            KeyCode::Char('f') if !paged => self.cycle_filter(),
            KeyCode::Char('t') if paged => self.cycle_range(),
            KeyCode::Char('u') if events_tab => {
                self.unacked_only = !self.unacked_only;
                let len = self.list_len(ListId::Events);
                self.events_cursor.set(0, len);
            }
            KeyCode::Char('a') if events_tab => self.ack_selected(),
            KeyCode::Char('a') if matches!(self.pages.last(), Some(Page::Event(_))) => {
                self.ack_page_event();
            }
            KeyCode::Char('A') if events_tab => self.ack_visible(),
            KeyCode::Char('r') if ctrl => self.refresh(),
            KeyCode::F(5) => self.refresh(),
            KeyCode::Char('r') => self.request(Action::Restart),
            KeyCode::Char('s') => self.request(Action::Start),
            KeyCode::Char('S') => self.request(Action::Stop),
            KeyCode::Char('R') => self.request(Action::Remonitor),
            KeyCode::Char('m') => self.request_monitor_toggle(),
            _ => {}
        }
    }

    // ---- mouse ---------------------------------------------------------

    fn on_mouse(&mut self, mouse: MouseEvent) {
        match self.mode {
            Mode::Help => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    self.mode = Mode::Normal;
                }
                return;
            }
            Mode::Confirm(_) => return,
            Mode::Search => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    self.mode = Mode::Normal;
                }
            }
            Mode::Normal => {}
        }
        let at = Position::new(mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click(at),
            MouseEventKind::Down(MouseButton::Right) => self.back(),
            MouseEventKind::ScrollDown => self.scroll(at, 1),
            MouseEventKind::ScrollUp => self.scroll(at, -1),
            _ => {}
        }
    }

    fn list_at(&self, at: Position) -> Option<ListHit> {
        self.hits
            .lists
            .iter()
            .find(|hit| hit.outer.contains(at))
            .copied()
    }

    fn click(&mut self, at: Position) {
        if let Some((tab, _)) = self.hits.tabs.iter().find(|(_, rect)| rect.contains(at)) {
            let tab = *tab;
            self.switch_tab(tab);
            return;
        }
        let Some(hit) = self.list_at(at) else {
            return;
        };
        // The first click on an unfocused overview list only focuses it.
        let was_focused = !matches!(hit.id, ListId::Attention | ListId::OverviewEvents)
            || self.overview_focus == hit.id;
        self.focus(hit.id);
        let first_row = hit.outer.y + 1 + u16::from(hit.header);
        let last_row = hit.outer.bottom().saturating_sub(1);
        if at.y < first_row || at.y >= last_row {
            return;
        }
        let len = self.list_len(hit.id);
        let Some(cursor) = self.cursor_mut(hit.id) else {
            return;
        };
        let index = cursor.state.offset() + usize::from(at.y - first_row);
        if index >= len {
            return;
        }
        let was_selected = cursor.state.selected() == Some(index);
        cursor.set(index, len);
        if was_selected && was_focused {
            self.drill();
        }
    }

    fn scroll(&mut self, at: Position, delta: isize) {
        let id = match self.list_at(at) {
            Some(hit) => {
                self.focus(hit.id);
                Some(hit.id)
            }
            None => self.active_list(),
        };
        if let Some(id) = id {
            let len = self.list_len(id);
            if let Some(cursor) = self.cursor_mut(id) {
                cursor.step(delta, len);
            }
        }
    }

    /// Focuses an overview pane; other lists need no focus.
    fn focus(&mut self, id: ListId) {
        if matches!(id, ListId::Attention | ListId::OverviewEvents) {
            self.overview_focus = id;
        }
    }

    // ---- navigation ----------------------------------------------------

    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.pages.clear();
    }

    /// Goes back one page; the view below keeps its selection and focus.
    fn back(&mut self) {
        if self.pages.pop().is_some() {
            self.reload_top();
            self.fetch_charts();
        }
    }

    fn nav(&mut self, nav: Nav) {
        let Some(id) = self.active_list() else {
            return;
        };
        let len = self.list_len(id);
        if let Some(cursor) = self.cursor_mut(id) {
            match nav {
                Nav::By(delta) => cursor.step(delta, len),
                Nav::First => cursor.set(0, len),
                Nav::Last => cursor.set(len.saturating_sub(1), len),
            }
        }
    }

    fn cycle_filter(&mut self) {
        match self.tab {
            Tab::Hosts => {
                self.host_filter = self.host_filter.next();
                let len = self.list_len(ListId::Hosts);
                self.hosts_cursor.set(0, len);
            }
            Tab::Services => {
                self.service_filter = self.service_filter.next();
                let len = self.list_len(ListId::Services);
                self.services_cursor.set(0, len);
            }
            Tab::Overview | Tab::Events => {}
        }
    }

    fn cycle_range(&mut self) {
        let Some(page) = self.pages.last_mut() else {
            return;
        };
        match page {
            Page::Host(p) => {
                p.range = p.range.next();
                p.charts = Charts::Loading;
            }
            Page::Service(p) => {
                p.range = p.range.next();
                p.charts = Charts::Loading;
            }
            Page::Event(_) => return,
        }
        self.fetch_charts();
    }

    pub fn push_host(&mut self, host_id: u64, name: String, select_service: Option<String>) {
        self.pages.push(Page::Host(Box::new(HostPage {
            host_id,
            name,
            data: None,
            cursor: Cursor::default(),
            error: None,
            charts: Charts::Loading,
            range: Range::default(),
            select_service,
        })));
        self.fetch_host(host_id);
    }

    pub fn push_service(&mut self, host_id: u64, host: String, service: String) {
        self.fetch_service(host_id, &service);
        self.pages.push(Page::Service(Box::new(ServicePage {
            host_id,
            host,
            service,
            data: None,
            cursor: Cursor::default(),
            error: None,
            charts: Charts::Loading,
            range: Range::default(),
        })));
    }

    pub fn push_event(&mut self, event: Event) {
        self.pages.push(Page::Event(Box::new(EventPage { event })));
    }

    /// Opens the page of the selected entry (Enter, Right, `l`, or a click on the selected row).
    fn drill(&mut self) {
        let Some(id) = self.active_list() else {
            return self.drill_from_event();
        };
        match id {
            ListId::Attention => {
                let Some(item) = self
                    .attention()
                    .into_iter()
                    .nth(self.attention_cursor.selected())
                else {
                    return;
                };
                match item.service {
                    Some(service) => self.push_service(item.host_id, item.host, service),
                    None => self.push_host(item.host_id, item.host, None),
                }
            }
            ListId::OverviewEvents => {
                if let Some(event) = self.events.get(self.overview_events_cursor.selected()) {
                    self.push_event(event.clone());
                }
            }
            ListId::Events => {
                let event = self
                    .visible_events()
                    .get(self.events_cursor.selected())
                    .map(|e| (*e).clone());
                if let Some(event) = event {
                    self.push_event(event);
                }
            }
            ListId::Hosts => {
                let host = self
                    .visible_hosts()
                    .get(self.hosts_cursor.selected())
                    .map(|h| (h.id, h.label().to_owned()));
                if let Some((id, name)) = host {
                    self.push_host(id, name, None);
                }
            }
            ListId::Services => {
                let service = self
                    .visible_services()
                    .get(self.services_cursor.selected())
                    .map(|s| (s.host_id, s.host.clone(), s.name.clone()));
                if let Some((host_id, host, name)) = service {
                    self.push_service(host_id, host, name);
                }
            }
            ListId::HostServices => {
                let Some(Page::Host(page)) = self.pages.last() else {
                    return;
                };
                let target = self
                    .detail_services()
                    .get(page.cursor.selected())
                    .map(|s| (page.host_id, page.name.clone(), s.name.clone()));
                if let Some((host_id, host, name)) = target {
                    self.push_service(host_id, host, name);
                }
            }
            ListId::ServiceEvents => {
                let Some(Page::Service(page)) = self.pages.last() else {
                    return;
                };
                let event = page
                    .data
                    .as_ref()
                    .and_then(|d| d.recent_events.get(page.cursor.selected()))
                    .cloned();
                if let Some(event) = event {
                    self.push_event(event);
                }
            }
        }
    }

    /// From an event page, jump to the service (or else the host) the event belongs to.
    fn drill_from_event(&mut self) {
        let Some(Page::Event(page)) = self.pages.last() else {
            return;
        };
        let event = &page.event;
        let host = event.host.clone().unwrap_or_default();
        match (event.host_id, event.service.clone()) {
            (Some(id), Some(service)) => self.push_service(id, host, service),
            (Some(id), None) => self.push_host(id, host, None),
            (None, _) => self.set_toast("This event has no host", Tone::Dim, 2),
        }
    }

    // ---- actions -------------------------------------------------------

    fn request(&mut self, action: Action) {
        let Some(target) = self.selected_service() else {
            return;
        };
        if self.read_only() {
            self.set_toast(
                "This token is read-only: it cannot run actions",
                Tone::Warn,
                4,
            );
            return;
        }
        self.mode = Mode::Confirm(Pending::Service {
            host_id: target.host_id,
            host: target.host,
            service: target.service,
            action,
        });
    }

    fn request_monitor_toggle(&mut self) {
        let Some(target) = self.selected_service() else {
            return;
        };
        let action = if target.state == ServiceState::Unmonitored {
            Action::Monitor
        } else {
            Action::Unmonitor
        };
        self.request(action);
    }

    fn ack_selected(&mut self) {
        let Some(event) = self
            .visible_events()
            .get(self.events_cursor.selected())
            .map(|e| (e.id, e.acked()))
        else {
            return;
        };
        self.ack_one(event.0, event.1);
    }

    fn ack_page_event(&mut self) {
        if let Some(Page::Event(page)) = self.pages.last() {
            let id = page.event.id;
            let acked = self.event_now(&page.event).acked();
            self.ack_one(id, acked);
        }
    }

    fn ack_one(&mut self, id: u64, acked: bool) {
        if acked {
            self.set_toast("Already acknowledged", Tone::Dim, 2);
        } else if self.read_only() {
            self.set_toast(
                "This token is read-only: it cannot acknowledge events",
                Tone::Warn,
                4,
            );
        } else {
            self.execute(Pending::Ack(vec![id]));
        }
    }

    fn ack_visible(&mut self) {
        let ids: Vec<u64> = self
            .visible_events()
            .iter()
            .filter(|e| !e.acked())
            .map(|e| e.id)
            .collect();
        if ids.is_empty() {
            self.set_toast("Nothing to acknowledge", Tone::Dim, 2);
        } else if self.read_only() {
            self.set_toast(
                "This token is read-only: it cannot acknowledge events",
                Tone::Warn,
                4,
            );
        } else {
            self.mode = Mode::Confirm(Pending::Ack(ids));
        }
    }

    fn execute(&mut self, pending: Pending) {
        match pending {
            Pending::Service {
                host_id,
                host,
                service,
                action,
            } => {
                let label = format!("{} {service} on {host}", action.as_str());
                let client = self.client.clone();
                self.spawn_done(label, Vec::new(), async move {
                    client
                        .service_action(host_id, &service, action.as_str())
                        .await
                });
            }
            Pending::Ack(ids) => {
                let label = format!("acknowledged {} event(s)", ids.len());
                let client = self.client.clone();
                let sent = ids.clone();
                self.spawn_done(label, ids, async move { client.ack(&sent).await });
            }
        }
    }

    fn spawn_done(
        &mut self,
        label: String,
        acked: Vec<u64>,
        request: impl Future<Output = Result<Value, ApiError>> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        self.set_toast(format!("… {label}"), Tone::Dim, 10);
        tokio::spawn(async move {
            let result = request.await.map(|_| ());
            let _ = tx.send(Msg::Done {
                label,
                acked,
                result,
            });
        });
    }
}

/// A host's services, worst first.
pub fn sorted_services(data: &HostDetail) -> Vec<&Service> {
    let mut services: Vec<&Service> = data.services.iter().collect();
    services.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then_with(|| a.name.cmp(&b.name))
    });
    services
}

#[derive(Debug, Clone, Copy)]
enum Nav {
    By(isize),
    First,
    Last,
}

/// Case-insensitive substring test; `needle` is already lowercase and may be empty.
fn contains(haystack: &str, needle: &str) -> bool {
    needle.is_empty() || haystack.to_lowercase().contains(needle)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::config::{Resolved, Secret, TokenSource};
    use ratatui::crossterm::event::KeyEventState;

    pub fn app() -> (App, tokio::sync::mpsc::UnboundedReceiver<Msg>) {
        let config = Resolved {
            url: "http://127.0.0.1:9".into(),
            token: TokenSource::Literal(Secret::new("mnr_test")),
            tls_skip_verify: false,
            timeout: Duration::from_secs(1),
        };
        let client = Client::new(&config, Secret::new("mnr_test")).unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(client, tx);
        app.hosts = vec![
            host(1, "web-1", HostState::Ok),
            host(2, "db-1", HostState::Offline),
            host(3, "cache", HostState::Degraded),
        ];
        app.services = vec![
            service(1, "web-1", "nginx", ServiceState::Ok),
            service(1, "web-1", "sshd", ServiceState::Failed),
            service(3, "cache", "redis", ServiceState::Unmonitored),
        ];
        app.overview.failing = vec![crate::types::ServiceRef {
            host_id: 1,
            host: "web-1".into(),
            service: "sshd".into(),
            kind: "process".into(),
            status_text: "Does not exist".into(),
            since: Some(crate::fmt::now() - 600.0),
        }];
        app.events = vec![
            event(2, "sshd is down", false),
            event(1, "nginx is up", true),
        ];
        app.loaded = true;
        (app, rx)
    }

    pub fn host(id: u64, name: &str, state: HostState) -> HostSummary {
        HostSummary {
            id,
            hostname: format!("{name}.example.com"),
            display_name: Some(name.to_owned()),
            state,
            online: state != HostState::Offline,
            last_seen: crate::fmt::now() - 30.0,
            services: crate::types::ServiceCounts {
                total: 4,
                ok: 3,
                failed: 1,
                ..Default::default()
            },
            system: crate::types::SystemStats {
                cpu: Some(12.0),
                mem_percent: Some(48.0),
                load: Some([0.4, 0.3, 0.2]),
                ..Default::default()
            },
            sparkline: crate::types::Sparklines {
                cpu: vec![5.0, 20.0, 60.0, 30.0, 12.0],
                mem: vec![40.0, 41.0, 48.0, 47.0, 48.0],
            },
            failing: if state == HostState::Degraded {
                vec!["redis".into()]
            } else {
                Vec::new()
            },
            ..Default::default()
        }
    }

    pub fn service(host_id: u64, host: &str, name: &str, state: ServiceState) -> ServiceListItem {
        ServiceListItem {
            host_id,
            host: host.to_owned(),
            name: name.to_owned(),
            kind: "process".to_owned(),
            state,
            status_text: if state == ServiceState::Failed {
                "Does not exist"
            } else {
                "Running"
            }
            .to_owned(),
            state_since: Some(crate::fmt::now() - 3600.0),
            ..Default::default()
        }
    }

    pub fn event(id: u64, message: &str, acked: bool) -> Event {
        Event {
            id,
            host_id: Some(1),
            host: Some("web-1".into()),
            service: Some("sshd".into()),
            state: crate::types::EventState::Failed,
            message: message.to_owned(),
            created_at: crate::fmt::now() - 120.0,
            acked_at: acked.then_some(1.0),
            ..Default::default()
        }
    }

    pub fn key(code: KeyCode) -> Msg {
        Msg::Input(TermEvent::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }

    pub fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            app.update(key(KeyCode::Char(c)));
        }
    }

    /// Opens a host page and feeds it a detail, as the API would.
    pub fn open_host(app: &mut App, id: u64, name: &str) {
        app.push_host(id, name.to_owned(), None);
        feed_host(app, id, name);
    }

    pub fn feed_host(app: &mut App, id: u64, name: &str) {
        let mut detail = HostDetail {
            summary: host(id, name, HostState::Ok),
            ..Default::default()
        };
        detail.services = vec![
            Service {
                name: "nginx".into(),
                kind: "process".into(),
                state: ServiceState::Ok,
                ..Default::default()
            },
            Service {
                name: "sshd".into(),
                kind: "process".into(),
                state: ServiceState::Failed,
                ..Default::default()
            },
            Service {
                name: name.into(),
                kind: "system".into(),
                state: ServiceState::Ok,
                ..Default::default()
            },
        ];
        app.update(Msg::Host(id, Ok(Box::new(detail))));
    }

    #[test]
    fn tabs_switch_with_numbers_and_tab() {
        let (mut app, _rx) = app();
        press(&mut app, "3");
        assert_eq!(app.tab, Tab::Services);
        app.update(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Events);
        app.update(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Overview);
        app.update(key(KeyCode::BackTab));
        assert_eq!(app.tab, Tab::Events);
    }

    #[test]
    fn hosts_are_sorted_worst_first_and_filterable() {
        let (mut app, _rx) = app();
        let names: Vec<_> = app
            .visible_hosts()
            .iter()
            .map(|h| h.label().to_owned())
            .collect();
        assert_eq!(names, ["db-1", "cache", "web-1"]);
        press(&mut app, "2f");
        assert_eq!(app.host_filter, HostFilter::Offline);
        assert_eq!(app.visible_hosts().len(), 1);
    }

    #[test]
    fn search_filters_and_esc_clears() {
        let (mut app, _rx) = app();
        press(&mut app, "3/ssh");
        assert_eq!(app.mode, Mode::Search);
        app.update(key(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.visible_services().len(), 1);
        app.update(key(KeyCode::Esc));
        assert_eq!(app.visible_services().len(), 3);
    }

    #[test]
    fn state_filter_cycles_on_services() {
        let (mut app, _rx) = app();
        press(&mut app, "3f");
        assert_eq!(app.service_filter, ServiceFilter::Failed);
        assert_eq!(app.visible_services().len(), 1);
        press(&mut app, "f");
        assert_eq!(app.service_filter, ServiceFilter::Unmonitored);
        assert_eq!(app.visible_services()[0].name, "redis");
    }

    #[test]
    fn service_action_asks_for_confirmation_and_can_be_cancelled() {
        let (mut app, _rx) = app();
        press(&mut app, "3j");
        press(&mut app, "r");
        assert_eq!(
            app.mode,
            Mode::Confirm(Pending::Service {
                host_id: 1,
                host: "web-1".into(),
                service: "sshd".into(),
                action: Action::Restart
            })
        );
        press(&mut app, "n");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn monitor_key_toggles_by_state() {
        let (mut app, _rx) = app();
        press(&mut app, "3jjm");
        let Mode::Confirm(Pending::Service {
            action, service, ..
        }) = &app.mode
        else {
            panic!("expected a confirmation, got {:?}", app.mode);
        };
        assert_eq!((service.as_str(), *action), ("redis", Action::Monitor));
    }

    #[test]
    fn viewers_get_a_toast_instead_of_a_prompt() {
        let (mut app, _rx) = app();
        app.update(Msg::Role(Some("viewer".into())));
        press(&mut app, "3r");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.toast.as_ref().unwrap().text.contains("read-only"));
    }

    #[test]
    fn actions_do_nothing_outside_a_service_list() {
        let (mut app, _rx) = app();
        press(&mut app, "2r");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn ack_all_visible_confirms_with_the_unacked_ids() {
        let (mut app, _rx) = app();
        press(&mut app, "4A");
        assert_eq!(app.mode, Mode::Confirm(Pending::Ack(vec![2])));
    }

    #[test]
    fn done_marks_events_acked_and_reports_forbidden() {
        let (mut app, _rx) = app();
        app.refreshing = true; // keep `on_done` from spawning a refresh
        app.update(Msg::Done {
            label: "ack".into(),
            acked: vec![2],
            result: Ok(()),
        });
        assert!(app.events[0].acked());
        assert_eq!(app.unacked_count(), 0);
        app.update(Msg::Done {
            label: "restart sshd".into(),
            acked: vec![],
            result: Err(ApiError::Forbidden("operator required".into())),
        });
        let toast = app.toast.as_ref().unwrap();
        assert!(toast.text.contains("permission denied"), "{}", toast.text);
        assert_eq!(toast.tone, Tone::Bad);
    }

    #[test]
    fn stream_updates_hosts_and_events() {
        let (mut app, _rx) = app();
        let mut updated = host(1, "web-1", HostState::Degraded);
        updated.services.failed = 2;
        app.update(Msg::Stream(StreamMessage::Host {
            host: Box::new(updated),
        }));
        assert_eq!(app.hosts[0].state, HostState::Degraded);
        app.update(Msg::Stream(StreamMessage::Host {
            host: Box::new(host(9, "new", HostState::Ok)),
        }));
        assert_eq!(app.hosts.len(), 4);
        app.update(Msg::Stream(StreamMessage::HostRemoved { id: 9 }));
        assert_eq!(app.hosts.len(), 3);
        app.update(Msg::Stream(StreamMessage::Event {
            event: Box::new(event(3, "new event", false)),
        }));
        assert_eq!(app.events[0].id, 3);
        // an update of a known event replaces it in place
        app.update(Msg::Stream(StreamMessage::Event {
            event: Box::new(event(3, "acked now", true)),
        }));
        assert_eq!(app.events.len(), 3);
        assert!(app.events[0].acked());
    }

    #[test]
    fn quit_and_help() {
        let (mut app, _rx) = app();
        press(&mut app, "?");
        assert_eq!(app.mode, Mode::Help);
        press(&mut app, "x");
        assert_eq!(app.mode, Mode::Normal);
        press(&mut app, "q");
        assert!(app.quit);
    }

    #[test]
    fn cursor_clamps() {
        let mut cursor = Cursor::default();
        cursor.step(5, 3);
        assert_eq!(cursor.selected(), 2);
        cursor.step(-9, 3);
        assert_eq!(cursor.selected(), 0);
        cursor.set(0, 0);
        assert_eq!(cursor.state.selected(), None);
    }

    #[test]
    fn attention_lists_hosts_then_services() {
        let (app, _rx) = app();
        let kinds: Vec<_> = app.attention().iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            [
                AttentionKind::Offline,
                AttentionKind::Degraded,
                AttentionKind::Failed
            ]
        );
    }

    #[test]
    fn overview_pane_focus_and_selection() {
        let (mut app, _rx) = app();
        assert_eq!(app.overview_focus, ListId::Attention);
        press(&mut app, "j");
        assert_eq!(app.attention_cursor.selected(), 1);
        press(&mut app, "p");
        assert_eq!(app.overview_focus, ListId::OverviewEvents);
        press(&mut app, "j");
        assert_eq!(app.overview_events_cursor.selected(), 1);
        assert_eq!(
            app.attention_cursor.selected(),
            1,
            "the other pane keeps its selection"
        );
        press(&mut app, "p");
        assert_eq!(app.overview_focus, ListId::Attention);
    }

    #[tokio::test]
    async fn enter_on_an_attention_entry_opens_the_right_page() {
        let (mut app, _rx) = app();
        // first entry: the offline host
        app.update(key(KeyCode::Enter));
        assert!(matches!(app.pages.last(), Some(Page::Host(p)) if p.host_id == 2));
        app.update(key(KeyCode::Left));
        assert!(app.pages.is_empty());
        // last entry: the failing service opens its service page
        press(&mut app, "jj");
        app.update(key(KeyCode::Right));
        assert!(
            matches!(app.pages.last(), Some(Page::Service(p)) if p.host_id == 1 && p.service == "sshd")
        );
    }

    #[tokio::test]
    async fn enter_on_an_overview_event_opens_the_event_page() {
        let (mut app, _rx) = app();
        press(&mut app, "pj");
        app.update(key(KeyCode::Enter));
        assert!(matches!(app.pages.last(), Some(Page::Event(p)) if p.event.id == 1));
    }

    #[tokio::test]
    async fn back_stack_restores_view_selection_and_focus() {
        let (mut app, _rx) = app();
        press(&mut app, "pj");
        assert_eq!(app.overview_events_cursor.selected(), 1);
        // overview event -> event page -> (jump) service page -> back -> back
        app.update(key(KeyCode::Enter));
        app.update(key(KeyCode::Char('l')));
        assert_eq!(app.pages.len(), 2);
        assert!(matches!(app.pages.last(), Some(Page::Service(_))));
        app.update(key(KeyCode::Char('h')));
        assert!(matches!(app.pages.last(), Some(Page::Event(_))));
        app.update(key(KeyCode::Esc));
        assert!(app.pages.is_empty());
        assert_eq!(app.tab, Tab::Overview);
        assert_eq!(app.overview_focus, ListId::OverviewEvents);
        assert_eq!(app.overview_events_cursor.selected(), 1);
    }

    #[tokio::test]
    async fn host_page_drills_into_a_service_and_back() {
        let (mut app, _rx) = app();
        press(&mut app, "2");
        app.update(key(KeyCode::Enter));
        assert!(matches!(app.pages.last(), Some(Page::Host(_))));
        // the host page starts empty; feed it the detail of the opened host
        feed_host(&mut app, 2, "db-1");
        press(&mut app, "j");
        app.update(key(KeyCode::Right));
        assert!(matches!(app.pages.last(), Some(Page::Service(_))));
        app.update(key(KeyCode::Left));
        let Some(Page::Host(page)) = app.pages.last() else {
            panic!("expected the host page");
        };
        assert_eq!(
            page.cursor.selected(),
            1,
            "the host page keeps its selection"
        );
    }

    #[tokio::test]
    async fn services_tab_enter_opens_a_service_page() {
        let (mut app, _rx) = app();
        press(&mut app, "3j");
        app.update(key(KeyCode::Enter));
        assert!(
            matches!(app.pages.last(), Some(Page::Service(p)) if p.host_id == 1 && p.service == "sshd")
        );
        // actions work on the service page too
        press(&mut app, "r");
        assert!(matches!(app.mode, Mode::Confirm(Pending::Service { .. })));
        press(&mut app, "n");
        app.update(key(KeyCode::Char('q')));
        assert!(app.pages.is_empty());
        assert!(!app.quit, "q goes back from a page instead of quitting");
    }

    #[tokio::test]
    async fn events_tab_enter_opens_the_event_page_and_ack_works_there() {
        let (mut app, _rx) = app();
        app.role = Some("viewer".into());
        press(&mut app, "4");
        app.update(key(KeyCode::Enter));
        assert!(matches!(app.pages.last(), Some(Page::Event(p)) if p.event.id == 2));
        press(&mut app, "a");
        assert!(app.toast.as_ref().unwrap().text.contains("read-only"));
    }

    #[tokio::test]
    async fn switching_tabs_clears_the_pages() {
        let (mut app, _rx) = app();
        press(&mut app, "3");
        app.update(key(KeyCode::Enter));
        assert_eq!(app.pages.len(), 1);
        press(&mut app, "2");
        assert!(app.pages.is_empty());
    }

    #[tokio::test]
    async fn range_key_cycles_on_pages_only() {
        let (mut app, _rx) = app();
        press(&mut app, "t");
        open_host(&mut app, 1, "web-1");
        press(&mut app, "t");
        let Some(Page::Host(page)) = app.pages.last() else {
            panic!()
        };
        assert_eq!(page.range, Range::Day);
    }
}
