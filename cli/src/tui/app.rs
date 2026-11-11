//! TUI state and its reaction to input, API results and live updates.
//!
//! Nothing here blocks: HTTP calls run in spawned tasks and report back with a
//! [`Msg`], so the UI thread only ever touches in-memory state.

use std::future::Future;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::widgets::TableState;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ApiError, Client, decode};
use crate::fmt::Tone;
use crate::types::{
    Action, Event, EventsResponse, HostDetail, HostState, HostSummary, Me, Overview, Service,
    ServiceListItem, ServiceState, ServicesResponse, StreamMessage,
};

pub type Tx = UnboundedSender<Msg>;

/// Everything the main loop reacts to.
pub enum Msg {
    Input(TermEvent),
    Tick,
    Snapshot(Result<Box<Snapshot>, ApiError>),
    Detail(u64, Result<Box<HostDetail>, ApiError>),
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

pub struct Detail {
    pub host_id: u64,
    pub name: String,
    pub data: Option<HostDetail>,
    pub cursor: Cursor,
    pub error: Option<String>,
    select_service: Option<String>,
}

#[cfg(test)]
impl Detail {
    pub fn for_test(host_id: u64, name: &str, data: HostDetail) -> Self {
        Self {
            host_id,
            name: name.to_owned(),
            data: Some(data),
            cursor: Cursor::default(),
            error: None,
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
    /// Search text per tab, indexed by [`Tab::index`].
    pub search: [String; 4],
    pub host_filter: HostFilter,
    pub service_filter: ServiceFilter,
    pub unacked_only: bool,
    pub detail: Option<Detail>,

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
            search: Default::default(),
            host_filter: HostFilter::default(),
            service_filter: ServiceFilter::default(),
            unacked_only: false,
            detail: None,
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

    /// The services of the open host, worst first.
    pub fn detail_services(&self) -> Vec<&Service> {
        let Some(data) = self.detail.as_ref().and_then(|d| d.data.as_ref()) else {
            return Vec::new();
        };
        let mut services: Vec<&Service> = data.services.iter().collect();
        services.sort_by(|a, b| {
            a.state
                .rank()
                .cmp(&b.state.rank())
                .then_with(|| a.name.cmp(&b.name))
        });
        services
    }

    fn current_len(&self) -> usize {
        if self.detail.is_some() {
            return self.detail_services().len();
        }
        match self.tab {
            Tab::Overview => 0,
            Tab::Hosts => self.visible_hosts().len(),
            Tab::Services => self.visible_services().len(),
            Tab::Events => self.visible_events().len(),
        }
    }

    fn current_cursor(&mut self) -> Option<&mut Cursor> {
        if let Some(detail) = self.detail.as_mut() {
            return Some(&mut detail.cursor);
        }
        match self.tab {
            Tab::Overview => None,
            Tab::Hosts => Some(&mut self.hosts_cursor),
            Tab::Services => Some(&mut self.services_cursor),
            Tab::Events => Some(&mut self.events_cursor),
        }
    }

    fn selected_service(&self) -> Option<ServiceTarget> {
        if let Some(detail) = &self.detail {
            let services = self.detail_services();
            let service = services.get(detail.cursor.selected())?;
            return Some(ServiceTarget {
                host_id: detail.host_id,
                host: detail.name.clone(),
                service: service.name.clone(),
                state: service.state,
            });
        }
        if self.tab != Tab::Services {
            return None;
        }
        let services = self.visible_services();
        let s = services.get(self.services_cursor.selected())?;
        Some(ServiceTarget {
            host_id: s.host_id,
            host: s.host.clone(),
            service: s.name.clone(),
            state: s.state,
        })
    }

    // ---- messages ------------------------------------------------------

    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Input(TermEvent::Key(key)) => self.on_key(key),
            Msg::Input(_) => {}
            Msg::Tick => self.on_tick(),
            Msg::Snapshot(result) => self.on_snapshot(result),
            Msg::Detail(id, result) => self.on_detail(id, result),
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
        if let Some(id) = self.detail.as_ref().map(|d| d.host_id) {
            self.fetch_detail(id);
        }
    }

    fn fetch_detail(&self, id: u64) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        tokio::spawn(async move {
            let result = client
                .host(id)
                .await
                .and_then(decode::<HostDetail>)
                .map(Box::new);
            let _ = tx.send(Msg::Detail(id, result));
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

    fn on_detail(&mut self, id: u64, result: Result<Box<HostDetail>, ApiError>) {
        let Some(detail) = self.detail.as_mut().filter(|d| d.host_id == id) else {
            return;
        };
        match result {
            Ok(data) => {
                detail.error = None;
                detail.data = Some(*data);
                if let Some(name) = detail.select_service.take() {
                    let index = self.detail_services().iter().position(|s| s.name == name);
                    let len = self.detail_services().len();
                    if let (Some(index), Some(detail)) = (index, self.detail.as_mut()) {
                        detail.cursor.set(index, len);
                    }
                }
            }
            Err(err) => detail.error = Some(err.to_string()),
        }
        let len = self.detail_services().len();
        if let Some(detail) = self.detail.as_mut() {
            let selected = detail.cursor.selected();
            detail.cursor.set(selected, len);
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
                if self.detail.as_ref().is_some_and(|d| d.host_id == id) {
                    self.detail = None;
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
                        if self.events_cursor.selected() > 0 {
                            let len = self.events.len();
                            self.events_cursor.step(1, len);
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
                for event in self.events.iter_mut().filter(|e| acked.contains(&e.id)) {
                    event.acked_at = Some(crate::fmt::now());
                    event.acked_by.get_or_insert_with(|| "you".to_owned());
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
        let len = self.current_len();
        if let Some(cursor) = self.current_cursor() {
            cursor.set(0, len);
        }
    }

    fn on_normal_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let in_detail = self.detail.is_some();
        match key.code {
            KeyCode::Char('q') if in_detail => self.detail = None,
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc if in_detail => self.detail = None,
            KeyCode::Esc => {
                let index = self.tab.index();
                self.search[index].clear();
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Tab => self.switch_tab(self.tab.step(1)),
            KeyCode::BackTab => self.switch_tab(self.tab.step(-1)),
            KeyCode::Char(c @ '1'..='4') => {
                self.switch_tab(Tab::ALL[usize::from(c as u8 - b'1')]);
            }
            KeyCode::Right | KeyCode::Char('l') if !in_detail => self.switch_tab(self.tab.step(1)),
            KeyCode::Left | KeyCode::Char('h') if !in_detail => self.switch_tab(self.tab.step(-1)),
            KeyCode::Down | KeyCode::Char('j') => self.nav(Nav::By(1)),
            KeyCode::Up | KeyCode::Char('k') => self.nav(Nav::By(-1)),
            KeyCode::PageDown => self.nav(Nav::By(10)),
            KeyCode::Char('d') if ctrl => self.nav(Nav::By(10)),
            KeyCode::PageUp => self.nav(Nav::By(-10)),
            KeyCode::Char('u') if ctrl => self.nav(Nav::By(-10)),
            KeyCode::Home | KeyCode::Char('g') => self.nav(Nav::First),
            KeyCode::End | KeyCode::Char('G') => self.nav(Nav::Last),
            KeyCode::Enter => self.activate(),
            KeyCode::Char('/') if !in_detail && self.tab != Tab::Overview => {
                self.mode = Mode::Search
            }
            KeyCode::Char('f') if !in_detail => self.cycle_filter(),
            KeyCode::Char('u') if !in_detail && self.tab == Tab::Events => {
                self.unacked_only = !self.unacked_only;
                let len = self.current_len();
                self.events_cursor.set(0, len);
            }
            KeyCode::Char('a') if !in_detail && self.tab == Tab::Events => self.ack_selected(),
            KeyCode::Char('A') if !in_detail && self.tab == Tab::Events => self.ack_visible(),
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

    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.detail = None;
    }

    fn nav(&mut self, nav: Nav) {
        let len = self.current_len();
        if let Some(cursor) = self.current_cursor() {
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
                let len = self.current_len();
                self.hosts_cursor.set(0, len);
            }
            Tab::Services => {
                self.service_filter = self.service_filter.next();
                let len = self.current_len();
                self.services_cursor.set(0, len);
            }
            Tab::Overview | Tab::Events => {}
        }
    }

    fn activate(&mut self) {
        if self.detail.is_some() {
            return;
        }
        let target = match self.tab {
            Tab::Overview => None,
            Tab::Hosts => self
                .visible_hosts()
                .get(self.hosts_cursor.selected())
                .map(|h| (h.id, h.label().to_owned(), None)),
            Tab::Services => self
                .visible_services()
                .get(self.services_cursor.selected())
                .map(|s| (s.host_id, s.host.clone(), Some(s.name.clone()))),
            Tab::Events => self
                .visible_events()
                .get(self.events_cursor.selected())
                .and_then(|e| {
                    Some((
                        e.host_id?,
                        e.host.clone().unwrap_or_default(),
                        e.service.clone(),
                    ))
                }),
        };
        if let Some((id, name, service)) = target {
            self.detail = Some(Detail {
                host_id: id,
                name,
                data: None,
                cursor: Cursor::default(),
                error: None,
                select_service: service,
            });
            self.fetch_detail(id);
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
        if event.1 {
            self.set_toast("Already acknowledged", Tone::Dim, 2);
        } else if self.read_only() {
            self.set_toast(
                "This token is read-only: it cannot acknowledge events",
                Tone::Warn,
                4,
            );
        } else {
            self.execute(Pending::Ack(vec![event.0]));
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

    fn key(code: KeyCode) -> Msg {
        Msg::Input(TermEvent::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            app.update(key(KeyCode::Char(c)));
        }
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
}
