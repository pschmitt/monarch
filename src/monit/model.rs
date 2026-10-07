//! Monit enums and the normalization of raw status data into the JSON shapes
//! documented in `docs/API.md`.

use serde_json::{Value, json};

use super::xml::{self, Service, int, num};

pub const SERVICE_TYPES: [&str; 9] = [
    "filesystem",
    "directory",
    "file",
    "process",
    "host",
    "system",
    "fifo",
    "program",
    "net",
];

pub fn service_type_name(t: i64) -> &'static str {
    usize::try_from(t)
        .ok()
        .and_then(|i| SERVICE_TYPES.get(i).copied())
        .unwrap_or("unknown")
}

/// Labels used by M/Monit's API for service types.
pub fn service_type_label(t: i64) -> &'static str {
    match t {
        0 => "Filesystem",
        1 => "Directory",
        2 => "File",
        3 => "Process",
        4 => "Host",
        5 => "System",
        6 => "Fifo",
        7 => "Program",
        8 => "Net",
        _ => "Unknown",
    }
}

pub const ACTIONS: [&str; 8] = [
    "ignore",
    "alert",
    "restart",
    "stop",
    "exec",
    "unmonitor",
    "start",
    "monitor",
];

pub fn action_name(a: i64) -> Option<&'static str> {
    usize::try_from(a)
        .ok()
        .and_then(|i| ACTIONS.get(i).copied())
}

pub const USER_ACTIONS: [&str; 5] = ["start", "stop", "restart", "monitor", "unmonitor"];

/// Monit event bits (`Event_Type` in monit's `event.h`), with the failure
/// label, the recovery label and a short identifier.
pub const EVENTS: &[(i64, &str, &str, &str)] = &[
    (0x1, "checksum", "Checksum failed", "Checksum succeeded"),
    (
        0x2,
        "resource",
        "Resource limit matched",
        "Resource limit succeeded",
    ),
    (0x4, "timeout", "Timeout", "Timeout recovery"),
    (0x8, "timestamp", "Timestamp failed", "Timestamp succeeded"),
    (0x10, "size", "Size failed", "Size succeeded"),
    (
        0x20,
        "connection",
        "Connection failed",
        "Connection succeeded",
    ),
    (
        0x40,
        "permission",
        "Permission failed",
        "Permission succeeded",
    ),
    (0x80, "uid", "UID failed", "UID succeeded"),
    (0x100, "gid", "GID failed", "GID succeeded"),
    (0x200, "nonexist", "Does not exist", "Exists"),
    (0x400, "invalid", "Invalid type", "Type succeeded"),
    (0x800, "data", "Data access error", "Data access succeeded"),
    (0x1000, "exec", "Execution failed", "Execution succeeded"),
    (
        0x2000,
        "fsflags",
        "Filesystem flags failed",
        "Filesystem flags succeeded",
    ),
    (0x4000, "icmp", "Ping failed", "Ping succeeded"),
    (0x8000, "content", "Content failed", "Content succeeded"),
    (
        0x10000,
        "instance",
        "Monit instance failed",
        "Monit instance changed",
    ),
    (0x20000, "action", "Action done", "Action done"),
    (0x40000, "pid", "PID failed", "PID succeeded"),
    (0x80000, "ppid", "PPID failed", "PPID succeeded"),
    (
        0x100000,
        "heartbeat",
        "Heartbeat failed",
        "Heartbeat succeeded",
    ),
    (0x200000, "status", "Status failed", "Status succeeded"),
    (0x400000, "uptime", "Uptime failed", "Uptime succeeded"),
    (0x800000, "link", "Link down", "Link up"),
    (0x1000000, "speed", "Speed failed", "Speed succeeded"),
    (
        0x2000000,
        "saturation",
        "Saturation exceeded",
        "Saturation succeeded",
    ),
    (
        0x4000000,
        "bytein",
        "Download bytes exceeded",
        "Download bytes succeeded",
    ),
    (
        0x8000000,
        "byteout",
        "Upload bytes exceeded",
        "Upload bytes succeeded",
    ),
    (
        0x10000000,
        "packetin",
        "Download packets exceeded",
        "Download packets succeeded",
    ),
    (
        0x20000000,
        "packetout",
        "Upload packets exceeded",
        "Upload packets succeeded",
    ),
    (0x40000000, "exist", "Exists", "Does not exist"),
];

/// Synthetic event bit used for "host stopped reporting" events.
pub const EVENT_HEARTBEAT: i64 = 0x100000;

pub fn event_kind(bits: i64) -> &'static str {
    EVENTS
        .iter()
        .find(|e| bits & e.0 != 0)
        .map(|e| e.1)
        .unwrap_or("unknown")
}

pub fn event_label(bits: i64, state: i64) -> String {
    match EVENTS.iter().find(|e| bits & e.0 != 0) {
        Some(e) if state == 1 => e.2.to_owned(),
        Some(e) => e.3.to_owned(),
        None => "Event".to_owned(),
    }
}

pub fn event_state_name(state: i64) -> &'static str {
    match state {
        0 => "succeeded",
        1 => "failed",
        2 => "changed",
        3 | 4 => "changed_not",
        _ => "init",
    }
}

pub fn event_state_from_name(s: &str) -> Option<i64> {
    match s {
        "succeeded" => Some(0),
        "failed" => Some(1),
        "changed" => Some(2),
        "changed_not" => Some(3),
        _ => None,
    }
}

const MONITOR_YES: i64 = 0x1;
const MONITOR_INIT: i64 = 0x2;
const MONITOR_WAITING: i64 = 0x4;

/// Our coarse service state: "ok" | "failed" | "unmonitored" | "pending" | "init".
pub fn service_state(status: i64, monitor: i64, pending_action: i64) -> &'static str {
    if monitor == 0 {
        "unmonitored"
    } else if monitor & MONITOR_INIT != 0 {
        "init"
    } else if status != 0 {
        "failed"
    } else if pending_action != 0 {
        "pending"
    } else {
        "ok"
    }
}

/// Status text in the style of Monit / M/Monit ("Running", "Does not exist", ...).
pub fn status_text(type_id: i64, status: i64, monitor: i64, pending_action: i64) -> String {
    let mut text = if monitor == 0 {
        "Not monitored".to_owned()
    } else if monitor & MONITOR_INIT != 0 {
        "Initializing".to_owned()
    } else if status != 0 {
        let labels: Vec<&str> = EVENTS
            .iter()
            .filter(|e| status & e.0 != 0)
            .map(|e| e.2)
            .collect();
        if labels.is_empty() {
            "Failed".to_owned()
        } else {
            labels.join(", ")
        }
    } else {
        match type_id {
            0 | 1 | 2 | 6 => "Accessible",
            3 => "Running",
            4 => "Online with all services",
            5 => "Running",
            7 => "Status ok",
            8 => "UP",
            _ => "OK",
        }
        .to_owned()
    };
    if pending_action != 0
        && let Some(a) = action_name(pending_action)
    {
        text.push_str(&format!(" - {a} pending"));
    }
    if monitor & MONITOR_WAITING != 0 && monitor & MONITOR_YES != 0 {
        text.push_str(" (Waiting)");
    }
    text
}

pub fn every_text(s: &Service) -> Option<String> {
    let every = s.every.as_ref()?;
    match int(&every.kind) {
        Some(2) => Some(format!(
            "Check every {} cycles",
            int(&every.number).unwrap_or(1)
        )),
        Some(3) => Some(format!(
            "Check every cron '{}'",
            every.cron.as_deref().unwrap_or("")
        )),
        Some(4) => Some(format!(
            "Don't check during cron '{}'",
            every.cron.as_deref().unwrap_or("")
        )),
        _ => None,
    }
}

fn response_ms(v: &Option<xml::Num>) -> Option<f64> {
    num(v).filter(|v| *v >= 0.0).map(|v| v * 1000.0)
}

fn ports(s: &Service) -> Value {
    Value::Array(
        s.port
            .iter()
            .map(|p| {
                json!({
                    "hostname": p.hostname.clone().unwrap_or_default(),
                    "port": int(&p.portnumber),
                    "protocol": p.protocol.clone().unwrap_or_default(),
                    "type": p.kind.clone().unwrap_or_default(),
                    "request": p.request.clone().filter(|r| !r.is_empty()),
                    "response_ms": response_ms(&p.responsetime),
                    "cert_valid_days": p.certificate.as_ref().and_then(|c| int(&c.valid)),
                })
            })
            .collect(),
    )
}

fn unix(s: &Service) -> Value {
    Value::Array(
        s.unix
            .iter()
            .map(|u| {
                json!({
                    "path": u.path.clone().unwrap_or_default(),
                    "protocol": u.protocol.clone().unwrap_or_default(),
                    "response_ms": response_ms(&u.responsetime),
                })
            })
            .collect(),
    )
}

fn icmp(s: &Service) -> Value {
    Value::Array(
        s.icmp
            .iter()
            .map(|i| {
                json!({
                    "type": i.kind.clone().unwrap_or_default(),
                    "response_ms": response_ms(&i.responsetime),
                })
            })
            .collect(),
    )
}

fn io(s: &Service) -> Value {
    let pick = |io: &Option<xml::IoStats>, ops: bool| {
        io.as_ref().and_then(|io| {
            let c = if ops {
                io.operations.as_ref()
            } else {
                io.bytes.as_ref().or(io.bytesgeneric.as_ref())
            };
            c.and_then(|c| num(&c.count))
        })
    };
    json!({
        "read_bps": pick(&s.read, false),
        "write_bps": pick(&s.write, false),
        "read_ops": pick(&s.read, true),
        "write_ops": pick(&s.write, true),
    })
}

fn timestamps(s: &Service) -> Value {
    s.timestamps.as_ref().map_or(
        Value::Null,
        |t| json!({"access": int(&t.access), "change": int(&t.change), "modify": int(&t.modify)}),
    )
}

fn system_cpu_total(c: &xml::SystemCpu) -> Option<f64> {
    // guest time is already accounted in user time on Linux
    let parts = [
        &c.user, &c.system, &c.nice, &c.wait, &c.hardirq, &c.softirq, &c.steal,
    ];
    let vals: Vec<f64> = parts.iter().filter_map(|p| num(p)).collect();
    if vals.is_empty() {
        None
    } else {
        Some((vals.iter().sum::<f64>()).min(100.0))
    }
}

/// Normalized, type specific service data.
pub fn service_data(s: &Service) -> Value {
    match s.type_id() {
        5 => {
            let sys = s.system.as_ref();
            let load = sys
                .and_then(|s| s.load.as_ref())
                .map(|l| json!([num(&l.avg01), num(&l.avg05), num(&l.avg15)]));
            let cpu = sys.and_then(|s| s.cpu.as_ref()).map(|c| {
                json!({
                    "user": num(&c.user), "system": num(&c.system), "nice": num(&c.nice),
                    "wait": num(&c.wait), "hardirq": num(&c.hardirq), "softirq": num(&c.softirq),
                    "steal": num(&c.steal), "guest": num(&c.guest), "guestnice": num(&c.guestnice),
                    "total": system_cpu_total(c),
                })
            });
            let mem = |m: Option<&xml::Memory>| {
                m.map(|m| json!({"percent": num(&m.percent), "kb": int(&m.kilobyte)}))
            };
            json!({
                "uptime": int(&s.uptime),
                "boottime": int(&s.boottime),
                "load": load,
                "cpu": cpu,
                "memory": mem(sys.and_then(|s| s.memory.as_ref())),
                "swap": mem(sys.and_then(|s| s.swap.as_ref())),
                "fd": s.filedescriptors.as_ref().map(|f| json!({
                    "allocated": int(&f.allocated), "unused": int(&f.unused), "maximum": int(&f.maximum),
                })),
            })
        }
        3 => json!({
            "pid": int(&s.pid),
            "ppid": int(&s.ppid),
            "uid": int(&s.uid),
            "euid": int(&s.euid),
            "gid": int(&s.gid),
            "uptime": int(&s.uptime),
            "threads": int(&s.threads),
            "children": int(&s.children),
            "cpu": s.cpu.as_ref().map(|c| json!({"percent": num(&c.percent), "percent_total": num(&c.percenttotal)})),
            "memory": s.memory.as_ref().map(|m| json!({
                "percent": num(&m.percent), "percent_total": num(&m.percenttotal),
                "kb": int(&m.kilobyte), "kb_total": int(&m.kilobytetotal),
            })),
            "fd": s.filedescriptors.as_ref().map(|f| json!({
                "open": int(&f.open), "open_total": int(&f.opentotal),
                "soft": f.limit.as_ref().and_then(|l| int(&l.soft)),
                "hard": f.limit.as_ref().and_then(|l| int(&l.hard)),
            })),
            "io": io(s),
            "ports": ports(s),
            "unix": unix(s),
        }),
        0 => json!({
            "fstype": s.fstype,
            "flags": s.fsflags,
            "mode": s.mode,
            "uid": int(&s.uid),
            "gid": int(&s.gid),
            "space": s.block.as_ref().map(|b| json!({
                "percent": num(&b.percent), "used_mb": num(&b.usage), "total_mb": num(&b.total),
            })),
            "inodes": s.inode.as_ref().map(|b| json!({
                "percent": num(&b.percent), "used": int(&b.usage), "total": int(&b.total),
            })),
            "io": io(s),
            "servicetime": s.servicetime.as_ref().map(|t| json!({
                "read": num(&t.read), "write": num(&t.write), "wait": num(&t.wait), "run": num(&t.run),
            })),
        }),
        1 | 2 | 6 => json!({
            "mode": s.mode,
            "uid": int(&s.uid),
            "gid": int(&s.gid),
            "size": int(&s.size),
            "hardlinks": int(&s.hardlink),
            "timestamps": timestamps(s),
            "checksum": s.checksum.as_ref().map(|c| json!({"type": c.kind, "value": c.value})),
        }),
        8 => {
            let dir = |d: Option<&xml::Direction>| {
                let nt = |x: Option<&xml::NowTotal>| {
                    (x.and_then(|x| num(&x.now)), x.and_then(|x| num(&x.total)))
                };
                let (p, pt) = nt(d.and_then(|d| d.packets.as_ref()));
                let (b, bt) = nt(d.and_then(|d| d.bytes.as_ref()));
                let (e, et) = nt(d.and_then(|d| d.errors.as_ref()));
                json!({
                    "packets": p, "bytes": b, "errors": e,
                    "packets_total": pt, "bytes_total": bt, "errors_total": et,
                })
            };
            let link = s.link.as_ref();
            json!({
                "link": link.map(|l| json!({
                    "state": int(&l.state), "speed": int(&l.speed), "duplex": int(&l.duplex),
                })),
                "download": dir(link.and_then(|l| l.download.as_ref())),
                "upload": dir(link.and_then(|l| l.upload.as_ref())),
            })
        }
        4 => json!({
            "icmp": icmp(s),
            "ports": ports(s),
            "unix": unix(s),
        }),
        7 => json!({
            "started": s.program.as_ref().and_then(|p| int(&p.started)),
            "exit_status": s.program.as_ref().and_then(|p| int(&p.status)),
            "output": s.program.as_ref().and_then(|p| p.output.clone()),
            "ports": ports(s),
        }),
        _ => json!({}),
    }
}

/// Extract numeric metrics worth keeping as time series.
pub fn metrics(s: &Service) -> Vec<(&'static str, f64)> {
    let mut out: Vec<(&'static str, Option<f64>)> = Vec::new();
    let io_pick = |io: &Option<xml::IoStats>| {
        io.as_ref().and_then(|io| {
            io.bytes
                .as_ref()
                .or(io.bytesgeneric.as_ref())
                .and_then(|c| num(&c.count))
        })
    };
    let first_port_ms = s
        .port
        .first()
        .and_then(|p| response_ms(&p.responsetime))
        .or_else(|| s.unix.first().and_then(|u| response_ms(&u.responsetime)));
    match s.type_id() {
        5 => {
            if let Some(sys) = &s.system {
                if let Some(l) = &sys.load {
                    out.push(("load1", num(&l.avg01)));
                    out.push(("load5", num(&l.avg05)));
                    out.push(("load15", num(&l.avg15)));
                }
                if let Some(c) = &sys.cpu {
                    out.push(("cpu", system_cpu_total(c)));
                    out.push(("cpu_user", num(&c.user)));
                    out.push(("cpu_system", num(&c.system)));
                    out.push(("cpu_wait", num(&c.wait)));
                    out.push(("cpu_nice", num(&c.nice)));
                    out.push(("cpu_steal", num(&c.steal)));
                }
                if let Some(m) = &sys.memory {
                    out.push(("mem_percent", num(&m.percent)));
                    out.push(("mem_kb", num(&m.kilobyte)));
                }
                if let Some(m) = &sys.swap {
                    out.push(("swap_percent", num(&m.percent)));
                    out.push(("swap_kb", num(&m.kilobyte)));
                }
            }
        }
        3 => {
            if let Some(c) = &s.cpu {
                out.push(("cpu", num(&c.percent)));
                out.push(("cpu_total", num(&c.percenttotal)));
            }
            if let Some(m) = &s.memory {
                out.push(("mem_percent", num(&m.percent)));
                out.push(("mem_kb", num(&m.kilobyte)));
                out.push(("mem_kb_total", num(&m.kilobytetotal)));
            }
            out.push(("threads", num(&s.threads)));
            out.push(("children", num(&s.children)));
            out.push((
                "fd_open",
                s.filedescriptors.as_ref().and_then(|f| num(&f.opentotal)),
            ));
            out.push(("read_bps", io_pick(&s.read)));
            out.push(("write_bps", io_pick(&s.write)));
            out.push(("port_ms", first_port_ms));
        }
        0 => {
            if let Some(b) = &s.block {
                out.push(("space_percent", num(&b.percent)));
                out.push(("space_used_mb", num(&b.usage)));
            }
            if let Some(i) = &s.inode {
                out.push(("inode_percent", num(&i.percent)));
            }
            out.push(("read_bps", io_pick(&s.read)));
            out.push(("write_bps", io_pick(&s.write)));
        }
        8 => {
            if let Some(l) = &s.link {
                let now =
                    |d: &Option<xml::Direction>,
                     f: fn(&xml::Direction) -> &Option<xml::NowTotal>| {
                        d.as_ref()
                            .and_then(|d| f(d).as_ref())
                            .and_then(|x| num(&x.now))
                    };
                out.push(("rx_bps", now(&l.download, |d| &d.bytes)));
                out.push(("tx_bps", now(&l.upload, |d| &d.bytes)));
                out.push(("rx_pps", now(&l.download, |d| &d.packets)));
                out.push(("tx_pps", now(&l.upload, |d| &d.packets)));
                out.push(("rx_errors", now(&l.download, |d| &d.errors)));
                out.push(("tx_errors", now(&l.upload, |d| &d.errors)));
            }
        }
        4 => {
            out.push((
                "icmp_ms",
                s.icmp.first().and_then(|i| response_ms(&i.responsetime)),
            ));
            out.push(("port_ms", first_port_ms));
        }
        2 => out.push(("size", num(&s.size))),
        7 => out.push((
            "exit_status",
            s.program.as_ref().and_then(|p| num(&p.status)),
        )),
        _ => {}
    }
    out.into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

/// "3d, 4h, 12m" style durations as used by M/Monit.
pub fn uptime_text(secs: i64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d, {hours}h, {mins}m")
    } else if hours > 0 {
        format!("{hours}h, {mins}m")
    } else {
        format!("{mins}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_texts() {
        assert_eq!(status_text(3, 0, 1, 0), "Running");
        assert_eq!(status_text(3, 0x200, 1, 0), "Does not exist");
        assert_eq!(status_text(7, 0x200000, 1, 0), "Status failed");
        assert_eq!(status_text(7, 0, 5, 0), "Status ok (Waiting)");
        assert_eq!(status_text(3, 0, 0, 0), "Not monitored");
        assert_eq!(service_state(0x200, 1, 0), "failed");
        assert_eq!(service_state(0, 0, 0), "unmonitored");
    }

    #[test]
    fn sample_metrics() {
        let m = xml::parse(include_bytes!("../../tests/fixtures/status.xml")).unwrap();
        let services = m.services.unwrap().services;
        let sys = services.iter().find(|s| s.type_id() == 5).unwrap();
        let ms = metrics(sys);
        assert!(ms.iter().any(|(k, _)| *k == "load1"));
        assert!(ms.iter().any(|(k, _)| *k == "cpu"));
        let data = service_data(sys);
        assert!(data["memory"]["percent"].is_number());
        for s in &services {
            let _ = service_data(s);
        }
    }
}
