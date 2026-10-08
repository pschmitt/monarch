//! HTML alert emails.
//!
//! Mail clients block scripts and external images and strip most SVG, so the
//! "graphs" are plain tables: gauge bars and bar sparklines. Everything is
//! inline-styled with table layout; a `prefers-color-scheme` block (honoured by
//! Apple Mail, Gmail apps, Outlook.com, ...) adds a dark theme.

use serde_json::Value;

use crate::notify::Notification;

const FONT: &str = "-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif";
const MONO: &str = "ui-monospace,SFMono-Regular,Menlo,Consolas,monospace";
const RED: &str = "#e11d48";
const GREEN: &str = "#10b981";
const BLUE: &str = "#3b82f6";
const AMBER: &str = "#d97706";
const ACCENT: &str = "#7c6cf0";

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// `2026-10-08 09:23:45 UTC` from unix seconds (no date crate needed).
pub fn fmt_utc(ts: f64) -> String {
    let secs = ts.floor() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn tone(state: &str) -> (&'static str, &'static str) {
    match state {
        "failed" => (RED, "Failed"),
        "succeeded" => (GREEN, "Recovered"),
        _ => (BLUE, "Changed"),
    }
}

fn pill(color: &str, text: &str) -> String {
    format!(
        "<span style=\"display:inline-block;padding:3px 10px;border-radius:999px;background:{color};color:#ffffff;font-size:11px;font-weight:700;letter-spacing:.06em;text-transform:uppercase;\">{}</span>",
        esc(text)
    )
}

/// A horizontal gauge: label, bar and value.
fn gauge(label: &str, percent: Option<f64>) -> String {
    let Some(p) = percent else {
        return String::new();
    };
    let p = p.clamp(0.0, 100.0);
    let color = if p >= 90.0 {
        RED
    } else if p >= 75.0 {
        AMBER
    } else {
        GREEN
    };
    format!(
        "<tr><td class=\"muted\" style=\"padding:4px 12px 4px 0;font-size:12px;color:#6b7189;width:70px;\">{}</td>\
         <td style=\"padding:4px 0;\"><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\"><tr>\
         <td class=\"track\" style=\"background:#e6e8f0;border-radius:4px;height:8px;line-height:8px;font-size:0;\">\
         <div style=\"width:{p:.0}%;min-width:3px;height:8px;background:{color};border-radius:4px;\">&nbsp;</div></td></tr></table></td>\
         <td style=\"padding:4px 0 4px 12px;font-family:{MONO};font-size:12px;width:48px;text-align:right;\">{p:.0}%</td></tr>",
        esc(label)
    )
}

/// A bar sparkline from raw samples, scaled to 0..=max(100, peak) for percentages.
pub fn sparkline(values: &[f64], color: &str, max: Option<f64>) -> String {
    if values.len() < 2 {
        return String::new();
    }
    let peak = max.unwrap_or_else(|| values.iter().cloned().fold(0.0, f64::max).max(1.0));
    let height = 36.0;
    let cells: String = values
        .iter()
        .map(|v| {
            let h = ((v / peak).clamp(0.0, 1.0) * height).round().max(1.0);
            format!(
                "<td valign=\"bottom\" style=\"padding:0 1px 0 0;font-size:0;line-height:0;\"><div style=\"width:4px;height:{h:.0}px;background:{color};border-radius:1px;\">&nbsp;</div></td>"
            )
        })
        .collect();
    format!(
        "<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" style=\"height:{height:.0}px;\"><tr>{cells}</tr></table>"
    )
}

fn row(label: &str, value: &str) -> String {
    format!(
        "<tr><td class=\"muted\" style=\"padding:5px 16px 5px 0;font-size:12px;color:#6b7189;white-space:nowrap;vertical-align:top;\">{}</td>\
         <td style=\"padding:5px 0;font-size:13px;\">{value}</td></tr>",
        esc(label)
    )
}

fn card(inner: &str) -> String {
    format!(
        "<tr><td style=\"padding:0 0 14px 0;\"><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" class=\"card\" style=\"background:#ffffff;border:1px solid #e3e5ee;border-radius:12px;\"><tr><td style=\"padding:18px 20px;\">{inner}</td></tr></table></td></tr>"
    )
}

fn samples(host: &Value, key: &str) -> Vec<f64> {
    host["sparkline"][key]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

/// The host card: status, resources with gauges, sparklines, failing checks.
fn host_card(host: &Value) -> String {
    let state = host["state"].as_str().unwrap_or("ok");
    let (color, label) = match state {
        "ok" => (GREEN, "Healthy"),
        "degraded" => (AMBER, "Degraded"),
        _ => (RED, "Offline"),
    };
    let name = host["display_name"]
        .as_str()
        .or(host["hostname"].as_str())
        .unwrap_or("host");
    let sys = &host["system"];
    let svc = &host["services"];
    let os = &host["os"];
    let os_line = [os["name"].as_str(), os["release"].as_str()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let load = sys["load"]
        .as_array()
        .map(|l| {
            l.iter()
                .filter_map(Value::as_f64)
                .map(|v| format!("{v:.2}"))
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .unwrap_or_default();
    let mut gauges = String::new();
    gauges.push_str(&gauge("CPU", sys["cpu"].as_f64()));
    gauges.push_str(&gauge("Memory", sys["mem_percent"].as_f64()));
    gauges.push_str(&gauge("Swap", sys["swap_percent"].as_f64()));
    let cpu = samples(host, "cpu");
    let mem = samples(host, "mem");
    let mut graphs = String::new();
    if cpu.len() > 1 {
        graphs.push_str(&format!(
            "<div class=\"muted\" style=\"margin:12px 0 4px;font-size:11px;color:#6b7189;\">CPU, recent</div>{}",
            sparkline(&cpu, ACCENT, Some(100.0))
        ));
    }
    if mem.len() > 1 {
        graphs.push_str(&format!(
            "<div class=\"muted\" style=\"margin:12px 0 4px;font-size:11px;color:#6b7189;\">Memory, recent</div>{}",
            sparkline(&mem, BLUE, Some(100.0))
        ));
    }
    let failing: Vec<&str> = host["failing"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let failing_html = if failing.is_empty() {
        String::new()
    } else {
        format!(
            "<div style=\"margin-top:14px;font-size:12px;color:{RED};font-weight:600;\">Failing checks</div>{}",
            failing
                .iter()
                .take(12)
                .map(|f| format!(
                    "<div style=\"font-family:{MONO};font-size:12px;padding:2px 0;\">{}</div>",
                    esc(f)
                ))
                .collect::<String>()
        )
    };
    let counts = format!(
        "{} ok · {} failed{}",
        svc["ok"].as_i64().unwrap_or(0),
        svc["failed"].as_i64().unwrap_or(0),
        match svc["unmonitored"].as_i64().unwrap_or(0) {
            0 => String::new(),
            n => format!(" · {n} unmonitored"),
        }
    );
    let uptime = sys["uptime"]
        .as_i64()
        .map(|s| format!("{}d {}h", s / 86400, s % 86400 / 3600))
        .unwrap_or_default();
    let mut meta = String::new();
    if !os_line.is_empty() {
        meta.push_str(&row("System", &esc(&os_line)));
    }
    meta.push_str(&row("Checks", &esc(&counts)));
    if !load.is_empty() {
        meta.push_str(&row(
            "Load",
            &format!("<span style=\"font-family:{MONO};\">{}</span>", esc(&load)),
        ));
    }
    if !uptime.is_empty() {
        meta.push_str(&row("Uptime", &esc(&uptime)));
    }
    card(&format!(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\"><tr>\
         <td style=\"font-size:15px;font-weight:700;\">{}</td><td align=\"right\">{}</td></tr></table>\
         <table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin-top:8px;\">{meta}</table>\
         <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin-top:8px;\">{gauges}</table>{graphs}{failing_html}",
        esc(name),
        pill(color, label)
    ))
}

fn button(url: &str, text: &str) -> String {
    format!(
        "<tr><td align=\"left\" style=\"padding:4px 0 18px 0;\"><a href=\"{}\" style=\"display:inline-block;background:{ACCENT};color:#ffffff;text-decoration:none;font-weight:600;font-size:14px;padding:11px 22px;border-radius:10px;\">{}</a></td></tr>",
        esc(url),
        esc(text)
    )
}

fn event_card(n: &Notification) -> String {
    let e = &n.event;
    let state = e["state"].as_str().unwrap_or("failed");
    let (color, label) = tone(state);
    let host = e["host"].as_str().unwrap_or("monarch");
    let service = e["service"].as_str();
    let mut meta = String::new();
    meta.push_str(&row("Host", &esc(host)));
    if let Some(s) = service {
        meta.push_str(&row(
            "Check",
            &format!("<span style=\"font-family:{MONO};\">{}</span>", esc(s)),
        ));
    }
    if let Some(t) = e["created_at"].as_f64() {
        meta.push_str(&row("When", &esc(&fmt_utc(t))));
    }
    if let Some(k) = e["kind_label"].as_str() {
        meta.push_str(&row("Event", &esc(k)));
    }
    let message = if n.text.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"msg\" style=\"margin:14px 0 4px;padding:12px 14px;border-left:4px solid {color};background:#f6f7fb;border-radius:6px;font-family:{MONO};font-size:12.5px;line-height:1.5;white-space:pre-wrap;word-break:break-word;\">{}</div>",
            esc(n.text.trim())
        )
    };
    card(&format!(
        "{}<div style=\"margin-top:10px;font-size:18px;font-weight:700;line-height:1.3;\">{}</div>{message}\
         <table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin-top:10px;\">{meta}</table>",
        pill(color, label),
        format!(
            "{} <span class=\"muted\" style=\"font-weight:400;color:#6b7189;\">· {}{}</span>",
            esc(e["kind_label"].as_str().unwrap_or("Event")),
            esc(host),
            service
                .map(|s| format!(" / {}", esc(s)))
                .unwrap_or_default()
        )
    ))
}

fn digest_card(n: &Notification) -> String {
    let events = n.event["events"].as_array().cloned().unwrap_or_default();
    let mut by_host: Vec<(String, Vec<&Value>)> = Vec::new();
    for e in &events {
        let h = e["host"].as_str().unwrap_or("monarch").to_owned();
        match by_host.iter_mut().find(|(k, _)| *k == h) {
            Some((_, v)) => v.push(e),
            None => by_host.push((h, vec![e])),
        }
    }
    let failed = events.iter().filter(|e| e["state"] == "failed").count();
    let recovered = events.iter().filter(|e| e["state"] == "succeeded").count();
    let summary = format!(
        "{} {} {}",
        pill(
            if failed > 0 { RED } else { GREEN },
            &format!("{} events", events.len())
        ),
        if failed > 0 {
            format!(
                "<span style=\"color:{RED};font-weight:600;font-size:13px;\">{failed} failed</span>"
            )
        } else {
            String::new()
        },
        if recovered > 0 {
            format!(
                "<span style=\"color:{GREEN};font-weight:600;font-size:13px;margin-left:8px;\">{recovered} recovered</span>"
            )
        } else {
            String::new()
        },
    );
    let mut body = String::new();
    for (host, items) in &by_host {
        body.push_str(&format!(
            "<div style=\"margin:16px 0 6px;font-size:14px;font-weight:700;\">{} <span class=\"muted\" style=\"font-weight:400;color:#6b7189;font-size:12px;\">· {}</span></div>",
            esc(host),
            items.len()
        ));
        for e in items.iter().take(15) {
            let (color, _) = tone(e["state"].as_str().unwrap_or("failed"));
            let when = e["created_at"].as_f64().map(fmt_utc).unwrap_or_default();
            let what = [e["service"].as_str(), e["kind_label"].as_str()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(": ");
            body.push_str(&format!(
                "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin-bottom:4px;\"><tr>\
                 <td width=\"10\" style=\"vertical-align:top;padding-top:6px;\"><div style=\"width:8px;height:8px;border-radius:4px;background:{color};\">&nbsp;</div></td>\
                 <td style=\"padding-left:6px;font-size:13px;\">{}<div class=\"muted\" style=\"font-size:11px;color:#6b7189;\">{} {}</div></td></tr></table>",
                esc(&what),
                esc(&when),
                esc(e["message"].as_str().unwrap_or("").lines().next().unwrap_or(""))
            ));
        }
        if items.len() > 15 {
            body.push_str(&format!(
                "<div class=\"muted\" style=\"font-size:12px;color:#6b7189;\">… and {} more</div>",
                items.len() - 15
            ));
        }
    }
    card(&format!("{summary}{body}"))
}

/// The full HTML document for a notification. `host` is the summary of the
/// event's host when there is exactly one.
pub fn render(n: &Notification, host: Option<&Value>, channel: &str) -> String {
    let digest = n.event["events"].is_array();
    let bar = if n.failed { RED } else { GREEN };
    let preheader = n.text.lines().next().unwrap_or("").trim();
    let mut body = String::new();
    body.push_str(&if digest {
        digest_card(n)
    } else {
        event_card(n)
    });
    if let Some(h) = host.filter(|_| !digest) {
        body.push_str(&host_card(h));
    }
    body.push_str(&button(
        &n.url,
        if digest {
            "Open events in Monarch"
        } else {
            "Open in Monarch"
        },
    ));
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"light dark\"><meta name=\"supported-color-schemes\" content=\"light dark\"><title>{title}</title>\
         <style>@media (prefers-color-scheme: dark){{body,.bg{{background:#0b0d12 !important;color:#e8eaf2 !important}}.card{{background:#141821 !important;border-color:#262b38 !important}}\
         .msg{{background:#0f1218 !important}}.muted{{color:#98a0b8 !important}}.track{{background:#262b38 !important}}}}\
         @media (max-width:620px){{.wrap{{width:100% !important}}}}</style></head>\
         <body class=\"bg\" style=\"margin:0;padding:0;background:#f4f5fa;color:#1f2430;font-family:{FONT};\">\
         <div style=\"display:none;max-height:0;overflow:hidden;opacity:0;\">{pre}</div>\
         <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" class=\"bg\" style=\"background:#f4f5fa;\"><tr><td align=\"center\" style=\"padding:24px 12px;\">\
         <table role=\"presentation\" class=\"wrap\" width=\"600\" cellpadding=\"0\" cellspacing=\"0\" style=\"width:600px;max-width:100%;\">\
         <tr><td style=\"height:5px;background:{bar};border-radius:5px 5px 0 0;font-size:0;line-height:0;\">&nbsp;</td></tr>\
         <tr><td style=\"padding:16px 4px 14px;\"><span style=\"font-size:18px;font-weight:800;letter-spacing:-.01em;\">&#9813; Monarch</span></td></tr>\
         {body}\
         <tr><td class=\"muted\" style=\"padding:6px 4px 0;font-size:11px;line-height:1.5;color:#6b7189;\">Sent by Monarch through the channel &ldquo;{channel}&rdquo;. Change what you get under Settings &rarr; Notifications.</td></tr>\
         </table></td></tr></table></body></html>",
        title = esc(&n.title),
        pre = esc(preheader),
        channel = esc(channel),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn note(event: Value, text: &str) -> Notification {
        Notification {
            title: "🔴 h · s: Status failed".into(),
            text: text.into(),
            url: "https://m.example/hosts/1".into(),
            failed: true,
            event,
        }
    }

    #[test]
    fn escapes_untrusted_text() {
        let n = note(
            json!({"state":"failed","host":"<b>h</b>","service":"s","kind_label":"x","created_at":0.0}),
            "<script>alert(1)</script>",
        );
        let html = render(&n, None, "a\"b");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;b&gt;h&lt;/b&gt;"));
        assert!(html.contains("a&quot;b"));
    }

    #[test]
    fn formats_utc() {
        assert_eq!(fmt_utc(0.0), "1970-01-01 00:00:00 UTC");
        assert_eq!(fmt_utc(1_791_450_233.9), "2026-10-08 09:03:53 UTC");
    }

    #[test]
    fn renders_host_card_with_graphs() {
        let n = note(
            json!({"state":"failed","host":"h","service":"s","kind_label":"Status failed","created_at":1.0e9}),
            "boom",
        );
        let host = json!({
            "state":"degraded","hostname":"h","failing":["s"],
            "system":{"cpu":93.0,"mem_percent":40.0,"swap_percent":null,"load":[0.1,0.2,0.3],"uptime":90000},
            "services":{"ok":3,"failed":1,"unmonitored":0},
            "os":{"name":"Linux","release":"6.12"},
            "sparkline":{"cpu":[1.0,50.0,90.0],"mem":[10.0,20.0,30.0]}
        });
        let html = render(&n, Some(&host), "mail");
        assert!(html.contains("Degraded"));
        assert!(html.contains("width:93%"), "cpu gauge");
        assert!(html.contains("CPU, recent"));
        assert!(html.contains("Failing checks"));
        assert!(html.contains("https://m.example/hosts/1"));
    }

    #[test]
    fn renders_digests_grouped_by_host() {
        let ev = |h: &str, st: &str| json!({"state":st,"host":h,"service":"s","kind_label":"Status","message":"m","created_at":1.0e9});
        let mut n = note(
            json!({"count":3,"events":[ev("a","failed"),ev("a","succeeded"),ev("b","failed")]}),
            "x",
        );
        n.title = "3 notifications".into();
        let html = render(&n, None, "mail");
        assert!(html.contains("3 events"));
        assert!(html.contains("2 failed"), "{html}");
        assert!(html.contains("1 recovered"), "{html}");
        assert!(html.contains("Open events in Monarch"));
    }

    #[test]
    fn sparkline_handles_short_series() {
        assert_eq!(sparkline(&[], RED, None), "");
        assert_eq!(sparkline(&[1.0], RED, None), "");
        assert!(sparkline(&[1.0, 2.0], RED, None).contains("<table"));
    }
}
