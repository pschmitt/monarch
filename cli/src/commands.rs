//! The non-interactive subcommands.

use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde_json::{Value, json};

use crate::api::{Client, decode};
use crate::cli::{EventsArgs, ServicesArgs, TargetsCommand};
use crate::duration;
use crate::fmt::{
    self, Cell, Palette, Table, Tone, ago, clock, event_badge, event_tone, host_badge, host_tone,
    service_badge, service_tone,
};
use crate::resolve::{self, Candidate};
use crate::types::{
    Action, Event, EventsResponse, HostDetail, HostState, HostSummary, MutedUntil, Overview,
    ServicesResponse, StreamMessage, Target,
};

pub struct Ctx {
    pub client: Client,
    pub json: bool,
    pub palette: Palette,
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

impl Ctx {
    fn paint(&self, tone: Tone, text: impl std::fmt::Display) -> String {
        self.palette.paint(tone, text)
    }

    async fn host(&self, query: &str) -> Result<HostSummary> {
        let hosts: Vec<HostSummary> = decode(self.client.hosts().await?)?;
        let candidates: Vec<Candidate> = hosts
            .iter()
            .map(|h| {
                Candidate::new(
                    h.id,
                    [
                        h.label(),
                        h.hostname.as_str(),
                        h.display_name.as_deref().unwrap_or(""),
                    ],
                )
            })
            .collect();
        let index = resolve::resolve(&candidates, query, "host")?;
        Ok(hosts
            .into_iter()
            .nth(index)
            .expect("index comes from the same list"))
    }
}

pub async fn status(ctx: &Ctx, check: bool) -> Result<ExitCode> {
    let (overview_raw, hosts_raw) = tokio::try_join!(ctx.client.overview(), ctx.client.hosts())?;
    let broken = {
        let overview: Overview = decode(overview_raw.clone())?;
        overview.hosts.offline > 0 || overview.hosts.degraded > 0 || overview.services.failed > 0
    };
    if ctx.json {
        print_json(&json!({ "overview": overview_raw, "hosts": hosts_raw }))?;
    } else {
        let overview: Overview = decode(overview_raw)?;
        let hosts: Vec<HostSummary> = decode(hosts_raw)?;
        print_status(ctx, &overview, &hosts);
    }
    Ok(if check && broken {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn print_status(ctx: &Ctx, o: &Overview, hosts: &[HostSummary]) {
    let count = |tone: Tone, n: u64, label: &str| {
        let text = format!("{n} {label}");
        if n == 0 {
            ctx.paint(Tone::Dim, text)
        } else {
            ctx.paint(tone, text)
        }
    };
    let sep = ctx.paint(Tone::Dim, " · ");
    println!(
        "{}",
        ctx.paint(
            Tone::Bold,
            format!(
                "Monarch  {}",
                ctx.client.base_url().as_str().trim_end_matches('/')
            )
        )
    );
    println!();
    println!(
        "{:<10}{:>4}   {}",
        "Hosts",
        o.hosts.total,
        [
            count(
                Tone::Good,
                o.hosts
                    .total
                    .saturating_sub(o.hosts.degraded + o.hosts.offline),
                "online"
            ),
            count(Tone::Warn, o.hosts.degraded, "degraded"),
            count(Tone::Bad, o.hosts.offline, "offline"),
        ]
        .join(&sep)
    );
    println!(
        "{:<10}{:>4}   {}",
        "Services",
        o.services.total,
        [
            count(Tone::Good, o.services.ok, "ok"),
            count(Tone::Bad, o.services.failed, "failed"),
            count(Tone::Warn, o.services.pending, "pending"),
            count(Tone::Dim, o.services.unmonitored, "unmonitored"),
        ]
        .join(&sep)
    );
    println!(
        "{:<10}{:>4}   {}",
        "Events",
        o.events_24h,
        ctx.paint(Tone::Dim, "in the last 24h")
    );

    let mut attention = Table::new(&[]);
    for host in hosts.iter().filter(|h| h.state == HostState::Offline) {
        attention.row(vec![
            Cell::new("✘ offline", Tone::Bad),
            Cell::plain(host.label()),
            Cell::dim(format!("last seen {}", ago(Some(host.last_seen)))),
        ]);
    }
    for host in hosts.iter().filter(|h| h.state == HostState::Degraded) {
        attention.row(vec![
            Cell::new("▲ degraded", Tone::Warn),
            Cell::plain(host.label()),
            Cell::dim(format!("failing: {}", host.failing.join(", "))),
        ]);
    }
    for s in &o.failing {
        attention.row(vec![
            Cell::new("✘ failed", Tone::Bad),
            Cell::plain(format!("{}/{}", s.host, s.service)),
            Cell::dim(format!("{}, {}", s.status_text, ago(s.since))),
        ]);
    }
    for s in &o.unmonitored {
        attention.row(vec![
            Cell::new("○ unmonitored", Tone::Dim),
            Cell::plain(format!("{}/{}", s.host, s.service)),
            Cell::dim(""),
        ]);
    }
    println!();
    if attention.is_empty() {
        println!("{}", ctx.paint(Tone::Good, "✔ All systems operational"));
    } else {
        println!("{}", ctx.paint(Tone::Bold, "Needs attention"));
        println!("{}", indent(&attention.render_rows(ctx.palette)));
    }

    if !o.recent_events.is_empty() {
        println!();
        println!("{}", ctx.paint(Tone::Bold, "Recent events"));
        let mut table = Table::new(&[]);
        for e in o.recent_events.iter().take(8) {
            table.row(event_row(e, false));
        }
        println!("{}", indent(&table.render_rows(ctx.palette)));
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn event_row(e: &Event, with_id: bool) -> Vec<Cell> {
    let mut row = Vec::new();
    if with_id {
        row.push(Cell::dim(e.id.to_string()));
    }
    let subject = match (&e.host, &e.service) {
        (Some(host), Some(service)) => format!("{host}/{service}"),
        (Some(host), None) => host.clone(),
        _ => "-".to_owned(),
    };
    row.extend([
        Cell::dim(clock(e.created_at)),
        Cell::new(event_badge(e.state), event_tone(e.state)),
        Cell::plain(subject),
        Cell::plain(e.message.clone()),
        Cell::new(if e.acked() { "✔ acked" } else { "" }, Tone::Dim),
    ]);
    row
}

pub async fn hosts(ctx: &Ctx) -> Result<()> {
    let raw = ctx.client.hosts().await?;
    if ctx.json {
        return print_json(&raw);
    }
    let hosts: Vec<HostSummary> = decode(raw)?;
    let mut table = Table::new(&[
        "STATE", "NAME", "SERVICES", "CPU", "MEM", "LOAD", "SEEN", "ID",
    ]);
    for h in &hosts {
        let failed = if h.services.failed > 0 {
            Cell::new(
                format!(
                    "{}/{} ({} failed)",
                    h.services.ok, h.services.total, h.services.failed
                ),
                Tone::Bad,
            )
        } else {
            Cell::plain(format!("{}/{}", h.services.ok, h.services.total))
        };
        let muted = if h.muted_until.is_some() {
            " (muted)"
        } else {
            ""
        };
        table.row(vec![
            Cell::new(host_badge(h.state), host_tone(h.state)),
            Cell::plain(format!("{}{muted}", h.label())),
            failed,
            Cell::plain(fmt::percent(h.system.cpu)),
            Cell::plain(fmt::percent(h.system.mem_percent)),
            Cell::plain(fmt::load(h.system.load)),
            Cell::dim(ago(Some(h.last_seen))),
            Cell::dim(h.id.to_string()),
        ]);
    }
    println!("{}", table.render(ctx.palette));
    Ok(())
}

pub async fn host(ctx: &Ctx, query: &str) -> Result<()> {
    let summary = ctx.host(query).await?;
    let raw = ctx.client.host(summary.id).await?;
    if ctx.json {
        return print_json(&raw);
    }
    let detail: HostDetail = decode(raw)?;
    let h = &detail.summary;
    println!(
        "{}  {}",
        ctx.paint(Tone::Bold, h.label()),
        ctx.paint(host_tone(h.state), host_badge(h.state))
    );
    let field = |name: &str, value: String| println!("  {:<10}{value}", ctx.paint(Tone::Dim, name));
    if h.label() != h.hostname {
        field("hostname", h.hostname.clone());
    }
    field("id", h.id.to_string());
    if let Some(description) = h.description.as_deref().filter(|d| !d.is_empty()) {
        field("about", description.to_owned());
    }
    let os = [&h.os.name, &h.os.release, &h.os.machine]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if !os.is_empty() {
        field("os", os);
    }
    field("last seen", ago(Some(h.last_seen)));
    if let Some(uptime) = h.system.uptime {
        field("uptime", fmt::short_duration(uptime));
    }
    field(
        "cpu / mem",
        format!(
            "{} / {}   load {}",
            fmt::percent(h.system.cpu),
            fmt::percent(h.system.mem_percent),
            fmt::load(h.system.load)
        ),
    );
    if let Some(url) = &detail.monit_url {
        field(
            "monit",
            format!("{url}{}", if h.can_act { "" } else { "  (cannot act)" }),
        );
    }
    if let Some(muted) = &h.muted_until {
        field("muted", describe_mute(muted));
    }
    println!();
    let mut services = detail.services;
    services.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut table = Table::new(&["STATE", "SERVICE", "TYPE", "STATUS", "SINCE"]);
    for s in &services {
        let status = match &s.pending_action {
            Some(action) => format!("{} (pending {action})", s.status_text),
            None => s.status_text.clone(),
        };
        table.row(vec![
            Cell::new(service_badge(s.state), service_tone(s.state)),
            Cell::plain(s.name.clone()),
            Cell::dim(s.kind.clone()),
            Cell::plain(status),
            Cell::dim(ago(s.state_since)),
        ]);
    }
    println!("{}", table.render(ctx.palette));
    Ok(())
}

fn describe_mute(muted: &MutedUntil) -> String {
    match muted {
        MutedUntil::Forever(_) => "until unmuted".to_owned(),
        MutedUntil::Until(ts) => format!("until {}", clock(*ts)),
    }
}

pub async fn services(ctx: &Ctx, args: &ServicesArgs) -> Result<()> {
    let host_id = match &args.host {
        Some(query) => Some(ctx.host(query).await?.id),
        None => None,
    };
    let raw = ctx
        .client
        .services(args.state.map(|s| s.as_str()), args.query.as_deref())
        .await?;
    let mut response: ServicesResponse = decode(raw.clone())?;
    if let Some(id) = host_id {
        response.services.retain(|s| s.host_id == id);
    }
    if ctx.json {
        let mut raw = raw;
        if let Some(id) = host_id
            && let Some(list) = raw.get_mut("services").and_then(Value::as_array_mut)
        {
            list.retain(|s| s.get("host_id").and_then(Value::as_u64) == Some(id));
        }
        return print_json(&raw);
    }
    let mut table = Table::new(&["STATE", "HOST", "SERVICE", "TYPE", "STATUS", "SINCE"]);
    for s in &response.services {
        let (badge, tone) = if s.stale {
            ("… stale", Tone::Dim)
        } else {
            (service_badge(s.state), service_tone(s.state))
        };
        table.row(vec![
            Cell::new(badge, tone),
            Cell::plain(s.host.clone()),
            Cell::plain(s.name.clone()),
            Cell::dim(s.kind.clone()),
            Cell::plain(s.status_text.clone()),
            Cell::dim(ago(s.state_since)),
        ]);
    }
    println!("{}", table.render(ctx.palette));
    Ok(())
}

pub async fn events(ctx: &Ctx, args: &EventsArgs) -> Result<()> {
    let host_id = match &args.host {
        Some(query) => Some(ctx.host(query).await?.id),
        None => None,
    };
    let raw = ctx.client.events(host_id, args.unacked, args.limit).await?;
    let mut response: EventsResponse = decode(raw.clone())?;
    // The API sends newest first; a log reads oldest first.
    response.events.reverse();
    if ctx.json {
        if args.follow {
            for event in raw["events"].as_array().into_iter().flatten().rev() {
                println!("{}", serde_json::to_string(event)?);
            }
        } else {
            print_json(&raw)?;
        }
    } else if !(args.follow && response.events.is_empty()) {
        let mut table = Table::new(&["ID", "TIME", "STATE", "SUBJECT", "MESSAGE", ""]);
        for e in &response.events {
            table.row(event_row(e, true));
        }
        println!("{}", table.render(ctx.palette));
    }
    if args.follow {
        follow(ctx, args, host_id).await?;
    }
    Ok(())
}

/// Prints events from `GET /api/stream` until interrupted, reconnecting when the stream drops.
async fn follow(ctx: &Ctx, args: &EventsArgs, host_id: Option<u64>) -> Result<()> {
    let mut backoff = Duration::from_secs(1);
    loop {
        match ctx.client.stream().await {
            Ok(mut stream) => {
                backoff = Duration::from_secs(1);
                loop {
                    match stream.next().await {
                        Ok(Some(value)) => print_stream_event(ctx, args, host_id, &value)?,
                        Ok(None) => {
                            eprintln!("{}", ctx.paint(Tone::Dim, "stream closed, reconnecting"));
                            break;
                        }
                        Err(err) => {
                            eprintln!(
                                "{}",
                                ctx.paint(Tone::Warn, format!("stream error: {err}, reconnecting"))
                            );
                            break;
                        }
                    }
                }
            }
            Err(err)
                if matches!(
                    err,
                    crate::api::ApiError::Unauthorized | crate::api::ApiError::Forbidden(_)
                ) =>
            {
                return Err(err.into());
            }
            Err(err) => {
                eprintln!(
                    "{}",
                    ctx.paint(Tone::Warn, format!("cannot open the stream: {err}"))
                );
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(15));
    }
}

fn print_stream_event(
    ctx: &Ctx,
    args: &EventsArgs,
    host_id: Option<u64>,
    value: &Value,
) -> Result<()> {
    let Ok(StreamMessage::Event { event }) = serde_json::from_value::<StreamMessage>(value.clone())
    else {
        return Ok(());
    };
    if host_id.is_some_and(|id| event.host_id != Some(id)) || (args.unacked && event.acked()) {
        return Ok(());
    }
    if ctx.json {
        println!("{}", serde_json::to_string(&value["event"])?);
    } else {
        let mut table = Table::new(&[]);
        table.row(event_row(&event, true));
        println!("{}", table.render_rows(ctx.palette));
    }
    Ok(())
}

pub async fn ack(ctx: &Ctx, ids: &[u64]) -> Result<()> {
    ctx.client.ack(ids).await?;
    if ctx.json {
        return print_json(&json!({ "acked": ids }));
    }
    let list: Vec<String> = ids.iter().map(u64::to_string).collect();
    println!(
        "{} acknowledged {}",
        ctx.paint(Tone::Good, "✔"),
        list.join(", ")
    );
    Ok(())
}

pub async fn action(ctx: &Ctx, query: &str, args: &[String]) -> Result<()> {
    let (verb, args) = args.split_last().context("missing action")?;
    let action = Action::from_str(verb, true).map_err(|_| {
        let names: Vec<&str> = Action::value_variants()
            .iter()
            .map(|a| a.as_str())
            .collect();
        anyhow::anyhow!(
            "unknown action {verb:?}, expected one of: {}",
            names.join(", ")
        )
    })?;
    let host = ctx.host(query).await?;
    let raw = if let [service] = args {
        ctx.client
            .service_action(host.id, service, action.as_str())
            .await?
    } else {
        ctx.client
            .bulk_action(host.id, args, action.as_str())
            .await?
    };
    if ctx.json {
        return print_json(&raw);
    }
    println!(
        "{} {} {} on {}",
        ctx.paint(Tone::Good, "✔"),
        action.as_str(),
        args.join(", "),
        host.label()
    );
    Ok(())
}

pub async fn mute(ctx: &Ctx, query: &str, duration: Option<&str>) -> Result<()> {
    let host = ctx.host(query).await?;
    let until = match duration {
        Some(text) => {
            let secs = duration::parse(text)?.as_secs();
            json!(fmt::now() as u64 + secs)
        }
        None => json!("forever"),
    };
    let raw = ctx.client.set_muted(host.id, until).await?;
    if ctx.json {
        return print_json(&raw);
    }
    let detail: HostDetail = decode(raw)?;
    let when = detail
        .summary
        .muted_until
        .as_ref()
        .map_or_else(|| "?".to_owned(), describe_mute);
    println!(
        "{} muted {} {when}",
        ctx.paint(Tone::Good, "✔"),
        host.label()
    );
    Ok(())
}

pub async fn unmute(ctx: &Ctx, query: &str) -> Result<()> {
    let host = ctx.host(query).await?;
    let raw = ctx.client.set_muted(host.id, Value::Null).await?;
    if ctx.json {
        return print_json(&raw);
    }
    println!("{} unmuted {}", ctx.paint(Tone::Good, "✔"), host.label());
    Ok(())
}

pub async fn targets(ctx: &Ctx, command: Option<&TargetsCommand>) -> Result<ExitCode> {
    match command {
        None => {
            let raw = ctx.client.targets().await?;
            if ctx.json {
                print_json(&raw)?;
                return Ok(ExitCode::SUCCESS);
            }
            let targets: Vec<Target> = decode(raw)?;
            let mut table = Table::new(&["ID", "NAME", "URL", "VIA", "EVERY", "STATUS", "POLLED"]);
            for t in &targets {
                let status = match (t.enabled, t.last_status.as_deref()) {
                    (false, _) => Cell::dim("disabled"),
                    (true, Some("ok")) => Cell::new("✔ ok", Tone::Good),
                    (true, Some(err)) => Cell::new(format!("✘ {err}"), Tone::Bad),
                    (true, None) => Cell::dim("not polled yet"),
                };
                table.row(vec![
                    Cell::dim(t.id.to_string()),
                    Cell::plain(t.name.clone()),
                    Cell::plain(t.url.clone()),
                    Cell::dim(
                        t.ssh
                            .as_ref()
                            .map_or("direct".to_owned(), |s| format!("ssh {}", s.destination)),
                    ),
                    Cell::dim(format!("{}s", t.interval)),
                    status,
                    Cell::dim(ago(t.last_polled_at)),
                ]);
            }
            println!("{}", table.render(ctx.palette));
            Ok(ExitCode::SUCCESS)
        }
        Some(TargetsCommand::Poll { target }) => {
            let targets: Vec<Target> = decode(ctx.client.targets().await?)?;
            let candidates: Vec<Candidate> = targets
                .iter()
                .map(|t| Candidate::new(t.id, [t.name.as_str()]))
                .collect();
            let index = resolve::resolve(&candidates, target, "target")?;
            let raw = ctx.client.poll_target(targets[index].id).await?;
            let polled: Target = decode(raw.clone())?;
            if ctx.json {
                print_json(&raw)?;
            } else {
                match polled.last_status.as_deref() {
                    Some("ok") => println!("{} polled {}", ctx.paint(Tone::Good, "✔"), polled.name),
                    other => println!(
                        "{} polling {} failed: {}",
                        ctx.paint(Tone::Bad, "✘"),
                        polled.name,
                        other.unwrap_or("no status")
                    ),
                }
            }
            Ok(if polled.last_status.as_deref() == Some("ok") {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
    }
}
