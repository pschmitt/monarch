//! Wire types of the Monarch HTTP API (see `docs/API.md`).
//!
//! Everything is tolerant: missing fields fall back to defaults and unknown
//! fields or enum values are ignored, so a newer or older server keeps working.

use std::collections::HashMap;

use clap::ValueEnum;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostState {
    Ok,
    Degraded,
    Offline,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Ok,
    Failed,
    Unmonitored,
    Pending,
    Init,
    #[default]
    #[serde(other)]
    Unknown,
}

impl ServiceState {
    /// Sort key: worst first.
    pub fn rank(self) -> u8 {
        match self {
            Self::Failed => 0,
            Self::Pending => 1,
            Self::Init => 2,
            Self::Unmonitored => 3,
            Self::Unknown => 4,
            Self::Ok => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventState {
    Succeeded,
    Failed,
    Changed,
    ChangedNot,
    Init,
    #[default]
    #[serde(other)]
    Unknown,
}

/// A service action understood by `POST .../action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Monitor,
    Unmonitor,
    Remonitor,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Monitor => "monitor",
            Self::Unmonitor => "unmonitor",
            Self::Remonitor => "remonitor",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ServiceCounts {
    pub total: u64,
    pub ok: u64,
    pub failed: u64,
    pub unmonitored: u64,
    pub pending: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HostCounts {
    pub total: u64,
    pub online: u64,
    pub offline: u64,
    pub degraded: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ServiceRef {
    pub host_id: u64,
    pub host: String,
    pub service: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub status_text: String,
    pub since: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ActivityBucket {
    pub ts: f64,
    pub ok: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Overview {
    pub hosts: HostCounts,
    pub services: ServiceCounts,
    pub events_24h: u64,
    pub failing: Vec<ServiceRef>,
    pub unmonitored: Vec<ServiceRef>,
    pub recent_events: Vec<Event>,
    pub activity: Vec<ActivityBucket>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Os {
    pub name: Option<String>,
    pub release: Option<String>,
    pub version: Option<String>,
    pub machine: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SystemStats {
    pub uptime: Option<f64>,
    pub cpu: Option<f64>,
    pub mem_percent: Option<f64>,
    pub mem_kb: Option<f64>,
    pub swap_percent: Option<f64>,
    pub load: Option<[f64; 3]>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Sparklines {
    pub cpu: Vec<f64>,
    pub mem: Vec<f64>,
}

/// `muted_until` is a unix timestamp, or the string `"forever"`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum MutedUntil {
    Until(f64),
    Forever(String),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HostSummary {
    pub id: u64,
    pub monit_id: String,
    pub hostname: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub state: HostState,
    pub online: bool,
    pub last_seen: f64,
    pub first_seen: f64,
    pub poll: u64,
    pub monit_version: Option<String>,
    pub monit_uptime: Option<f64>,
    pub os: Os,
    pub cpu_count: Option<u32>,
    pub mem_total_kb: Option<u64>,
    pub hostgroups: Vec<String>,
    pub services: ServiceCounts,
    pub system: SystemStats,
    pub sparkline: Sparklines,
    pub failing: Vec<String>,
    pub muted_services: Vec<String>,
    pub muted_until: Option<MutedUntil>,
    pub can_act: bool,
    pub source: Option<String>,
}

impl HostSummary {
    /// The name shown to humans: the display name, else the Monit hostname.
    pub fn label(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or(&self.hostname)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Service {
    pub id: u64,
    pub host_id: u64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub state: ServiceState,
    pub status_text: String,
    pub monitor_mode: String,
    pub pending_action: Option<String>,
    pub state_since: Option<f64>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HostDetail {
    #[serde(flatten)]
    pub summary: HostSummary,
    pub monit_url: Option<String>,
    pub remote_addr: Option<String>,
    pub services: Vec<Service>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ServiceListItem {
    pub host_id: u64,
    pub host: String,
    pub host_state: HostState,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub state: ServiceState,
    pub stale: bool,
    pub status_text: String,
    pub state_since: Option<f64>,
    pub pending_action: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ServicesResponse {
    pub services: Vec<ServiceListItem>,
    pub counts: HashMap<String, u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Event {
    pub id: u64,
    pub host_id: Option<u64>,
    pub host: Option<String>,
    pub service: Option<String>,
    pub kind: String,
    pub kind_label: String,
    pub state: EventState,
    pub action: Option<String>,
    pub message: String,
    pub created_at: f64,
    pub source: String,
    pub acked_by: Option<String>,
    pub acked_at: Option<f64>,
}

impl Event {
    pub fn acked(&self) -> bool {
        self.acked_at.is_some() || self.acked_by.is_some()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct EventsResponse {
    pub events: Vec<Event>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TargetSsh {
    pub destination: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Target {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub ssh: Option<TargetSsh>,
    pub interval: u64,
    pub enabled: bool,
    pub managed: bool,
    pub host_id: Option<u64>,
    pub last_status: Option<String>,
    pub last_polled_at: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct User {
    pub id: u64,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Me {
    pub user: Option<User>,
    pub version: String,
}

/// One message of `GET /api/stream`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamMessage {
    Host {
        host: Box<HostSummary>,
    },
    HostRemoved {
        id: u64,
    },
    Event {
        event: Box<Event>,
    },
    Ping,
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_summary_tolerates_missing_and_unknown_fields() {
        let host: HostSummary = serde_json::from_value(json!({
            "id": 3,
            "hostname": "web-1.example.com",
            "display_name": null,
            "state": "degraded",
            "online": true,
            "last_seen": 1700000000,
            "services": {"total": 5, "ok": 4, "failed": 1, "unmonitored": 0, "pending": 0},
            "system": {"cpu": 12.5, "load": [0.1, 0.2, 0.3], "mem_percent": 40},
            "sparkline": {"cpu": [1, 2.5, 3], "mem": []},
            "muted_until": "forever",
            "brand_new_field": {"x": 1}
        }))
        .unwrap();
        assert_eq!(host.id, 3);
        assert_eq!(host.state, HostState::Degraded);
        assert_eq!(host.label(), "web-1.example.com");
        assert_eq!(host.services.failed, 1);
        assert_eq!(host.system.load, Some([0.1, 0.2, 0.3]));
        assert_eq!(host.sparkline.cpu, vec![1.0, 2.5, 3.0]);
        assert_eq!(
            host.muted_until,
            Some(MutedUntil::Forever("forever".into()))
        );
    }

    #[test]
    fn muted_until_accepts_a_timestamp() {
        let host: HostSummary = serde_json::from_value(json!({"muted_until": 1700000123})).unwrap();
        assert_eq!(host.muted_until, Some(MutedUntil::Until(1_700_000_123.0)));
    }

    #[test]
    fn unknown_states_do_not_fail() {
        let host: HostSummary = serde_json::from_value(json!({"state": "sleeping"})).unwrap();
        assert_eq!(host.state, HostState::Unknown);
        let svc: ServiceListItem = serde_json::from_value(json!({"state": "exotic"})).unwrap();
        assert_eq!(svc.state, ServiceState::Unknown);
    }

    #[test]
    fn overview_sample() {
        let overview: Overview = serde_json::from_value(json!({
            "hosts": {"total": 3, "online": 2, "offline": 1, "degraded": 1},
            "services": {"total": 10, "ok": 8, "failed": 1, "unmonitored": 1, "pending": 0},
            "events_24h": 7,
            "failing": [{"host_id": 1, "host": "a", "service": "nginx", "type": "process",
                         "status_text": "Does not exist", "since": 1700000000}],
            "unmonitored": [],
            "top_cpu": [],
            "recent_events": [{"id": 9, "host_id": 1, "host": "a", "service": "nginx",
                               "service_type": "process", "kind": "nonexist",
                               "kind_label": "Does not exist", "state": "failed", "action": "alert",
                               "message": "boom", "created_at": 1700000000.5, "source": "monit",
                               "acked_by": null, "acked_at": null}],
            "activity": [{"ts": 1700000000, "ok": 1, "failed": 2}]
        }))
        .unwrap();
        assert_eq!(overview.hosts.offline, 1);
        assert_eq!(overview.failing[0].kind, "process");
        assert_eq!(overview.recent_events[0].state, EventState::Failed);
        assert!(!overview.recent_events[0].acked());
        assert_eq!(overview.activity[0].failed, 2);
    }

    #[test]
    fn host_detail_flattens_the_summary() {
        let detail: HostDetail = serde_json::from_value(json!({
            "id": 4, "hostname": "db", "state": "ok",
            "monit_url": "http://db:2812",
            "services": [{"id": 1, "host_id": 4, "name": "mysql", "type": "process",
                          "state": "ok", "status_text": "Running", "pending_action": null,
                          "state_since": null, "monitor_mode": "active", "data": {"pid": 1}}]
        }))
        .unwrap();
        assert_eq!(detail.summary.id, 4);
        assert_eq!(detail.services[0].name, "mysql");
        assert_eq!(detail.monit_url.as_deref(), Some("http://db:2812"));
    }

    #[test]
    fn services_and_events_responses() {
        let r: ServicesResponse = serde_json::from_value(json!({
            "services": [{"host_id": 1, "host": "a", "host_state": "offline", "name": "ssh",
                          "type": "process", "type_id": 3, "state": "ok", "stale": true,
                          "status_text": "Running", "state_since": null, "pending_action": null}],
            "counts": {"ok": 1, "stale": 1}
        }))
        .unwrap();
        assert!(r.services[0].stale);
        assert_eq!(r.services[0].host_state, HostState::Offline);
        assert_eq!(r.counts["stale"], 1);
        let e: EventsResponse =
            serde_json::from_value(json!({"events": [], "has_more": true})).unwrap();
        assert!(e.has_more);
    }

    #[test]
    fn stream_messages() {
        let host: StreamMessage =
            serde_json::from_str(r#"{"type":"host","host":{"id":2,"hostname":"x"}}"#).unwrap();
        assert!(matches!(host, StreamMessage::Host { host } if host.id == 2));
        let gone: StreamMessage =
            serde_json::from_str(r#"{"type":"host_removed","id":5}"#).unwrap();
        assert!(matches!(gone, StreamMessage::HostRemoved { id: 5 }));
        let ping: StreamMessage = serde_json::from_str(r#"{"type":"ping","ts":1.5}"#).unwrap();
        assert!(matches!(ping, StreamMessage::Ping));
        let other: StreamMessage = serde_json::from_str(r#"{"type":"future","a":1}"#).unwrap();
        assert!(matches!(other, StreamMessage::Other));
        let ev: StreamMessage = serde_json::from_str(
            r#"{"type":"event","event":{"id":1,"state":"changed_not","message":"m"}}"#,
        )
        .unwrap();
        assert!(
            matches!(ev, StreamMessage::Event { event } if event.state == EventState::ChangedNot)
        );
    }

    #[test]
    fn target_and_me() {
        let t: Target = serde_json::from_value(json!({
            "id": 1, "name": "rofl-10", "url": "http://127.0.0.1:2812",
            "ssh": {"destination": "root@rofl-10", "port": 22}, "interval": 30,
            "enabled": true, "managed": false, "host_id": 7, "last_status": "ok",
            "last_polled_at": 1700000000, "has_password": true
        }))
        .unwrap();
        assert_eq!(t.ssh.unwrap().port, Some(22));
        let me: Me = serde_json::from_value(json!({
            "user": {"id": 1, "username": "a", "role": "viewer"}, "setup_required": false,
            "version": "0.1.0", "oidc": null, "password_login": true
        }))
        .unwrap();
        assert_eq!(me.user.unwrap().role, "viewer");
    }
}
