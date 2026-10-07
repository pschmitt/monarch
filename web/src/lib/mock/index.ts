// Mock backend used when built with VITE_MOCK=1. Never imported otherwise.
import type { MockTransport } from "../api";
import type {
  Channel,
  EventState,
  HostDetail,
  HostSummary,
  MetricPoint,
  MonarchEvent,
  Overview,
  Service,
  ServiceState,
  ServiceType,
  Settings,
  StreamMessage,
  User,
} from "../types";

// ------------------------------------------------------------------ prng

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}
function rng(seed: number) {
  let a = seed || 1;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const now = () => Date.now() / 1000;
const r2 = (n: number, d = 1) => Math.round(n * 10 ** d) / 10 ** d;

const TYPE_ID: Record<ServiceType, number> = {
  filesystem: 0,
  directory: 1,
  file: 2,
  process: 3,
  host: 4,
  system: 5,
  fifo: 6,
  program: 7,
  net: 8,
};

// ------------------------------------------------------------------ fleet

interface HostSpec {
  name: string;
  arch: string;
  os: string;
  release: string;
  cpus: number;
  memGb: number;
  groups: string[];
  offline?: boolean;
  extraProcs?: string[];
  failing?: Record<string, { text: string; output?: string; exit?: number }>;
  unmonitored?: string[];
  pending?: string[];
  load: number; // base cpu %
  mem: number; // base mem %
}

const PROCS_COMMON = ["monit", "sshd", "tailscaled", "systemd-journald", "nscd", "chronyd"];

const SPECS: HostSpec[] = [
  {
    name: "rofl-10",
    arch: "x86_64",
    os: "Linux",
    release: "7.2.9",
    cpus: 8,
    memGb: 32,
    groups: ["rofl", "docker"],
    extraProcs: ["dockerd", "containerd", "nginx", "postgresql", "redis", "hermes-agent", "signal-cli", "vaultwarden", "immich-server", "n8n", "paperless", "jellyfin", "authelia", "sonarr", "radarr", "prowlarr"],
    failing: {
      "systemd services": {
        text: "Status failed",
        exit: 1,
        output: "🚨 Failed systemd services detected:\nrbw-auto-sync.service",
      },
    },
    unmonitored: ["jellyfin"],
    load: 14,
    mem: 38,
  },
  {
    name: "rofl-11",
    arch: "x86_64",
    os: "Linux",
    release: "7.2.9",
    cpus: 4,
    memGb: 16,
    groups: ["rofl"],
    extraProcs: ["nginx", "dockerd", "containerd", "gitea", "postgresql"],
    load: 3,
    mem: 11,
  },
  {
    name: "rofl-12",
    arch: "x86_64",
    os: "Linux",
    release: "7.2.9",
    cpus: 4,
    memGb: 16,
    groups: ["rofl", "docker"],
    extraProcs: ["dockerd", "containerd", "nginx", "home-assistant-proxy", "mosquitto"],
    pending: ["mosquitto"],
    load: 6,
    mem: 49,
  },
  {
    name: "rofl-13",
    arch: "x86_64",
    os: "Linux",
    release: "7.2.9",
    cpus: 32,
    memGb: 64,
    groups: ["rofl", "builders"],
    extraProcs: ["nix-daemon", "dockerd", "containerd", "browserless", "attic"],
    load: 22,
    mem: 9,
  },
  {
    name: "rofl-14",
    arch: "x86_64",
    os: "Linux",
    release: "7.2.8",
    cpus: 32,
    memGb: 64,
    groups: ["rofl", "builders"],
    extraProcs: ["nix-daemon", "dockerd", "browserless"],
    offline: true,
    load: 4,
    mem: 18,
  },
  {
    name: "oci-01",
    arch: "aarch64",
    os: "Linux",
    release: "7.2.9",
    cpus: 2,
    memGb: 12,
    groups: ["oci", "edge"],
    extraProcs: ["nginx", "dockerd", "containerd", "adguard", "wireguard"],
    load: 3,
    mem: 10,
  },
  {
    name: "oci-03",
    arch: "aarch64",
    os: "Linux",
    release: "7.2.9",
    cpus: 4,
    memGb: 24,
    groups: ["oci"],
    extraProcs: ["nginx", "mmonit", "restic-rest-server", "postgresql"],
    load: 6,
    mem: 43,
  },
  {
    name: "fnuc",
    arch: "x86_64",
    os: "Linux",
    release: "7.2.8",
    cpus: 12,
    memGb: 32,
    groups: ["home", "hypervisor"],
    extraProcs: ["libvirtd", "qemu-hass", "zigbee2mqtt", "mosquitto", "frigate", "go2rtc", "nginx"],
    failing: {
      "reboot required": {
        text: "Status failed",
        exit: 1,
        output:
          "Your kernel/systemd version changed. Reboot is advised.\nlinux-7.2.8 ➡️ linux-7.2.9\nlinux-7.2.8-modules ➡️ linux-7.2.9-modules\ninitrd-linux-7.2.8 ➡️ initrd-linux-7.2.9",
      },
    },
    load: 9,
    mem: 75,
  },
  {
    name: "turris",
    arch: "armv7l",
    os: "Linux",
    release: "5.15.148",
    cpus: 2,
    memGb: 2,
    groups: ["home", "network"],
    extraProcs: ["dnsmasq", "odhcpd", "hostapd", "kresd", "lighttpd", "mjpg-streamer", "modemmanager"],
    failing: {
      "mjpg-streamer": { text: "Does not exist" },
    },
    load: 27,
    mem: 30,
  },
];

interface MockHost {
  summary: HostSummary;
  detail: HostDetail;
}

function makeServices(spec: HostSpec, hostId: number, t: number): Service[] {
  const rand = rng(hash(spec.name));
  const out: Service[] = [];
  let sid = hostId * 1000;
  const add = (name: string, type: ServiceType, data: Record<string, any>, groups: string[] = []) => {
    const f = spec.failing?.[name];
    let state: ServiceState = "ok";
    let status_text = okText(type);
    if (f) {
      state = "failed";
      status_text = f.text;
    } else if (spec.unmonitored?.includes(name)) {
      state = "unmonitored";
      status_text = "Not monitored";
    } else if (spec.pending?.includes(name)) {
      state = "pending";
      status_text = "Restart pending";
    }
    if (spec.offline && state === "ok") status_text = okText(type);
    out.push({
      id: ++sid,
      host_id: hostId,
      name,
      type,
      type_id: TYPE_ID[type],
      state,
      status_text,
      status: state === "failed" ? (type === "process" ? 0x200 : 0x200000) : 0,
      monitor: state === "unmonitored" ? 0 : 1,
      monitor_mode: "active",
      pending_action: state === "pending" ? "restart" : null,
      every: name === "reboot required" ? "every 120 cycles" : name.includes("backup") ? "cron 0 3 * * *" : null,
      collected_at: spec.offline ? t - 3600 * 5 : t - rand() * 50,
      state_since: state === "failed" ? t - 3600 * (2 + rand() * 30) : t - 86400 * (1 + rand() * 20),
      groups,
      events: Math.floor(rand() * 60) + (state === "failed" ? 40 : 0),
      data,
    });
  };

  // system
  const memTotal = spec.memGb * 1024 * 1024;
  const cpuUser = spec.load * 0.62;
  const cpuSys = spec.load * 0.28;
  const cpuWait = spec.load * 0.06;
  add(spec.name, "system", {
    uptime: 86400 * (3 + rand() * 40),
    boottime: t - 86400 * 10,
    load: [r2((spec.load / 100) * spec.cpus * 1.1, 2), r2((spec.load / 100) * spec.cpus, 2), r2((spec.load / 100) * spec.cpus * 0.9, 2)],
    cpu: { user: r2(cpuUser), system: r2(cpuSys), nice: 0, wait: r2(cpuWait), hardirq: 0.1, softirq: 0.3, steal: 0, guest: 0, guestnice: 0, total: r2(spec.load) },
    memory: { percent: spec.mem, kb: Math.round((memTotal * spec.mem) / 100) },
    swap: { percent: spec.name === "fnuc" ? 12.4 : 0, kb: spec.name === "fnuc" ? 1_048_576 : 0 },
    fd: { allocated: Math.floor(4000 + rand() * 20000), unused: 0, maximum: 9223372036854775807 },
  });

  // processes
  const procs = [...PROCS_COMMON, ...(spec.extraProcs ?? [])];
  for (const p of procs) {
    const heavy = /postgres|immich|frigate|qemu|nix-daemon|jellyfin|dockerd|browserless|home-assistant/.test(p);
    const cpu = heavy ? r2(2 + rand() * 18) : r2(rand() * 1.5);
    const memKb = Math.round((heavy ? 300_000 + rand() * 2_400_000 : 8_000 + rand() * 120_000) * (spec.memGb / 16));
    const dead = spec.failing?.[p]?.text === "Does not exist";
    add(
      p,
      "process",
      dead
        ? {}
        : {
            pid: Math.floor(500 + rand() * 90000),
            ppid: 1,
            uid: p === "monit" || p === "sshd" ? 0 : 900 + Math.floor(rand() * 100),
            euid: 0,
            gid: 0,
            uptime: 3600 * (1 + rand() * 600),
            threads: Math.floor(1 + rand() * (heavy ? 120 : 12)),
            children: Math.floor(rand() * (heavy ? 30 : 3)),
            cpu: { percent: cpu, percent_total: r2(cpu * 1.2) },
            memory: { percent: r2((memKb / memTotal) * 100), percent_total: r2((memKb / memTotal) * 110), kb: memKb, kb_total: Math.round(memKb * 1.1) },
            fd: { open: Math.floor(10 + rand() * 600), open_total: Math.floor(20 + rand() * 900), soft: 1048576, hard: 1048576 },
            io: { read_bps: Math.round(rand() * 400_000), write_bps: Math.round(rand() * 900_000), read_ops: Math.round(rand() * 80), write_ops: Math.round(rand() * 120) },
            ports:
              p === "nginx"
                ? [{ hostname: "127.0.0.1", port: 443, protocol: "HTTP", type: "TCP", request: "/", response_ms: r2(1 + rand() * 8, 3), cert_valid_days: 61 }]
                : p === "postgresql"
                  ? [{ hostname: "127.0.0.1", port: 5432, protocol: "PGSQL", type: "TCP", request: null, response_ms: r2(0.3 + rand(), 3), cert_valid_days: null }]
                  : p === "adguard"
                    ? [{ hostname: "10.5.0.5", port: 15353, protocol: "DNS", type: "UDP", request: null, response_ms: 5.322, cert_valid_days: null }]
                    : [],
            unix: p === "monit" ? [] : [],
          },
      [p === "monit" ? "monit" : heavy ? "apps" : "base"],
    );
  }

  // programs
  add("systemd services", "program", {
    started: t - 30,
    exit_status: spec.failing?.["systemd services"] ? 1 : 0,
    output: spec.failing?.["systemd services"]?.output ?? "✅ No failed systemd services detected.",
  }, ["systemd"]);
  add("systemd timers", "program", { started: t - 30, exit_status: 0, output: "✅ No failed systemd timer targets detected." }, ["systemd"]);
  add("reboot required", "program", {
    started: t - 1200,
    exit_status: spec.failing?.["reboot required"] ? 1 : 0,
    output: spec.failing?.["reboot required"]?.output ?? "Reboot not required",
  }, ["nixos"]);
  if (spec.name.startsWith("rofl") || spec.name.startsWith("oci")) {
    add("restic backup", "program", { started: t - 3600 * 7, exit_status: 0, output: "snapshot 3f1c9a2e saved\nAdded to the repository: 182.442 MiB (61.007 MiB stored)\nprocessed 48213 files, 21.390 GiB in 2:41" }, ["backup"]);
  }

  // filesystems
  const fss: [string, string, number, number][] = [
    ["rootfs", "btrfs", 256 * 1024, spec.name === "fnuc" ? 81.4 : 30 + rand() * 40],
    ["boot", "vfat", 1024, 40 + rand() * 45],
  ];
  if (spec.name === "rofl-10" || spec.name === "rofl-13") fss.push(["data", "xfs", 4 * 1024 * 1024, spec.name === "rofl-10" ? 92.7 : 58.2]);
  if (spec.name === "turris") fss.splice(0, fss.length, ["rootfs", "btrfs", 7400, 46.3], ["msata-ssd", "xfs", 238_000, 2.2]);
  for (const [name, fstype, totalMb, percent] of fss) {
    add(name, "filesystem", {
      fstype,
      flags: "rw,relatime,ssd,discard=async,space_cache=v2",
      mode: "755",
      uid: 0,
      gid: 0,
      space: { percent: r2(percent), used_mb: r2((totalMb * percent) / 100), total_mb: totalMb },
      inodes: fstype === "vfat" || fstype === "btrfs" ? null : { percent: 0.4, used: 27488, total: 125028864 },
      io: { read_bps: Math.round(rand() * 5_000_000), write_bps: Math.round(rand() * 9_000_000), read_ops: r2(rand() * 40), write_ops: r2(rand() * 120) },
      servicetime: { read: r2(rand(), 3), write: r2(rand() * 2, 3), wait: 0, run: r2(rand() * 3, 3) },
    }, ["storage"]);
  }

  // net
  add("main-nic", "net", {
    link: { state: 1, speed: spec.name === "turris" ? 1_000_000_000 : 10_000_000_000, duplex: 1 },
    download: { packets: Math.round(rand() * 4000), bytes: Math.round(rand() * 4_000_000), errors: 0, packets_total: 6871178852, bytes_total: 5788824451893, errors_total: 0 },
    upload: { packets: Math.round(rand() * 3000), bytes: Math.round(rand() * 2_400_000), errors: 0, packets_total: 5861952232, bytes_total: 4786511961365, errors_total: 0 },
  }, ["network"]);
  add("tailscale0", "net", {
    link: { state: 1, speed: -1, duplex: -1 },
    download: { packets: Math.round(rand() * 200), bytes: Math.round(rand() * 200_000), errors: 0, packets_total: 99112, bytes_total: 812_211_090, errors_total: 0 },
    upload: { packets: Math.round(rand() * 200), bytes: Math.round(rand() * 120_000), errors: 0, packets_total: 81121, bytes_total: 412_119_122, errors_total: 0 },
  }, ["network"]);

  // remote hosts
  add("tailscale-magicdns", "host", { icmp: [{ type: "Ping", response_ms: r2(0.4 + rand(), 3) }], ports: [], unix: [] }, ["network"]);
  add(`${spec.name}-https`, "host", {
    icmp: [],
    ports: [{ hostname: `${spec.name}.brkn.lol`, port: 443, protocol: "HTTP", type: "TCP", request: "/healthz", response_ms: r2(20 + rand() * 70, 3), cert_valid_days: Math.floor(20 + rand() * 60) }],
    unix: [],
  }, ["web"]);
  add("monit.service", "host", { icmp: [], ports: [{ hostname: "127.0.0.1", port: 2812, protocol: "HTTP", type: "TCP", request: "/", response_ms: 4.9, cert_valid_days: null }], unix: [] }, ["monit"]);

  // files / dirs / fifo
  add("/etc/passwd", "file", { mode: "644", uid: 0, gid: 0, size: 2861, hardlinks: 1, timestamps: { access: t - 400, change: t - 86400 * 3, modify: t - 86400 * 3 }, checksum: { type: "SHA1", value: "8f2c1d6b0e4a9b3c7d5e1f0a2b4c6d8e0f1a3b5c" } }, ["security"]);
  add("/var/lib/acme", "directory", { mode: "750", uid: 0, gid: 0, size: null, hardlinks: 12, timestamps: { access: t - 900, change: t - 86400 * 9, modify: t - 86400 * 9 }, checksum: null }, ["security"]);
  if (spec.name === "fnuc") add("/run/hass.fifo", "fifo", { mode: "600", uid: 0, gid: 0, hardlinks: 1, timestamps: { access: t - 100, change: t - 86400, modify: t - 100 } });

  return out;
}

function okText(type: ServiceType): string {
  switch (type) {
    case "process":
      return "Running";
    case "program":
      return "Status ok";
    case "system":
      return "OK";
    case "host":
      return "Online with all services";
    case "net":
      return "Link up";
    default:
      return "Accessible";
  }
}

function counts(svcs: Service[]) {
  const c = { total: svcs.length, ok: 0, failed: 0, unmonitored: 0, pending: 0 };
  for (const s of svcs) {
    if (s.state === "ok") c.ok++;
    else if (s.state === "failed") c.failed++;
    else if (s.state === "unmonitored") c.unmonitored++;
    else c.pending++;
  }
  return c;
}

function buildFleet(): MockHost[] {
  const t = now();
  return SPECS.map((spec, i) => {
    const id = 101 + i * 7;
    const rand = rng(hash(spec.name + "h"));
    const services = makeServices(spec, id, t);
    const sys = services.find((s) => s.type === "system")!.data;
    const c = counts(services);
    const failing = services.filter((s) => s.state === "failed").map((s) => s.name);
    const state = spec.offline ? "offline" : failing.length ? "degraded" : "ok";
    const spark = (base: number, amp: number, seed: string) => {
      const rr = rng(hash(seed));
      return Array.from({ length: 60 }, (_, k) => Math.max(0, Math.min(100, base + Math.sin(k / 6 + rr() * 2) * amp + (rr() - 0.5) * amp)));
    };
    const summary: HostSummary = {
      id,
      monit_id: hash(spec.name).toString(16).padStart(8, "0").repeat(4),
      hostname: spec.name,
      display_name: null,
      description: spec.name === "fnuc" ? "Home hypervisor — runs the Home Assistant VM" : null,
      state,
      online: !spec.offline,
      last_seen: spec.offline ? t - 3600 * 5 - 120 : t - rand() * 40,
      first_seen: t - 86400 * 400,
      poll: 60,
      monit_version: "5.35.2",
      monit_uptime: 3600 * 9.6,
      os: { name: spec.os, release: spec.release, version: "#1-NixOS SMP PREEMPT_DYNAMIC Sat Oct  3 10:43:16 UTC 2026", machine: spec.arch },
      cpu_count: spec.cpus,
      mem_total_kb: spec.memGb * 1024 * 1024,
      swap_total_kb: spec.name === "fnuc" ? 8 * 1024 * 1024 : 0,
      hostgroups: spec.groups,
      services: c,
      system: {
        uptime: sys.uptime,
        cpu: sys.cpu.total,
        cpu_user: sys.cpu.user,
        cpu_system: sys.cpu.system,
        cpu_wait: sys.cpu.wait,
        mem_percent: sys.memory.percent,
        mem_kb: sys.memory.kb,
        swap_percent: sys.swap.percent,
        swap_kb: sys.swap.kb,
        load: sys.load,
      },
      sparkline: { cpu: spark(spec.load, spec.load * 0.4 + 2, spec.name + "c"), mem: spark(spec.mem, 1.5, spec.name + "m") },
      failing,
      muted_until: null,
      can_act: spec.name !== "turris",
    };
    const groups: Record<string, string[]> = {};
    for (const s of services) for (const g of s.groups) (groups[g] ??= []).push(s.name);
    const detail: HostDetail = {
      ...summary,
      controlfile: "/etc/monitrc",
      incarnation: Math.floor(t - 3600 * 9.6),
      startdelay: 0,
      httpd: { address: spec.name === "turris" ? null : `100.64.0.${10 + i}`, port: 2812, ssl: false, unixsocket: null },
      remote_addr: `100.64.0.${10 + i}`,
      monit_url: spec.name === "turris" ? null : `http://100.64.0.${10 + i}:2812`,
      override_url: null,
      override_username: null,
      has_override_password: false,
      has_reported_credentials: spec.name !== "turris",
      tls_skip_verify: true,
      servicegroups: groups,
      services,
    };
    return { summary, detail };
  });
}

// ------------------------------------------------------------------ events

const KINDS: [string, string][] = [
  ["nonexist", "Does not exist"],
  ["status", "Status failed"],
  ["connection", "Connection failed"],
  ["resource", "Resource limit matched"],
  ["action", "Action done"],
  ["instance", "Monit instance changed"],
  ["heartbeat", "No report from Monit"],
  ["pid", "PID changed"],
];

function buildEvents(fleet: MockHost[]): MonarchEvent[] {
  const rand = rng(42);
  const t = now();
  const out: MonarchEvent[] = [];
  let id = 90000;
  for (let i = 0; i < 400; i++) {
    const h = fleet[Math.floor(rand() * fleet.length)].detail;
    const s = h.services[Math.floor(rand() * h.services.length)];
    const [kind, label] = KINDS[Math.floor(rand() * KINDS.length)];
    const failed = rand() < 0.45;
    const state: EventState = kind === "action" || kind === "pid" || kind === "instance" ? "changed" : failed ? "failed" : "succeeded";
    const msg =
      kind === "nonexist"
        ? state === "failed"
          ? `process is not running`
          : `process is running with pid ${Math.floor(rand() * 90000)}`
        : kind === "status"
          ? state === "failed"
            ? `status failed (1) -- ${s.type === "program" ? (s.data.output ?? "").split("\n")[0] : "check failed"}`
            : "status succeeded (0)"
          : kind === "connection"
            ? state === "failed"
              ? `failed protocol test [HTTP] at [${h.hostname}.brkn.lol]:443 [TCP/IP TLS] -- HTTP error: Server returned status 502`
              : `connection succeeded to [${h.hostname}.brkn.lol]:443 [TCP/IP TLS]`
            : kind === "resource"
              ? state === "failed"
                ? `cpu usage of ${r2(80 + rand() * 19)}% matches resource limit [cpu usage > 80.0%]`
                : `cpu usage check succeeded [current cpu usage = ${r2(rand() * 20)}%]`
              : kind === "action"
                ? `restart action done`
                : kind === "instance"
                  ? "Monit reloaded"
                  : kind === "pid"
                    ? `process PID changed to ${Math.floor(rand() * 90000)}`
                    : state === "failed"
                      ? `No report from Monit for ${Math.floor(3 + rand() * 30)} minutes`
                      : "Monit is reporting again";
    out.push({
      id: id--,
      host_id: h.id,
      host: h.hostname,
      service: kind === "heartbeat" || kind === "instance" ? null : s.name,
      service_type: kind === "heartbeat" || kind === "instance" ? null : s.type,
      kind,
      kind_label: label,
      state,
      action: state === "failed" ? (rand() < 0.3 ? "restart" : "alert") : "alert",
      message: msg,
      created_at: t - i * (60 + rand() * 900) - rand() * 60,
      source: kind === "heartbeat" ? "monarch" : "monit",
      acked_by: rand() < 0.3 ? "pschmitt" : null,
      acked_at: null,
    });
  }
  // Make the currently failing services the newest events.
  let k = 0;
  for (const h of fleet) {
    for (const s of h.detail.services.filter((x) => x.state === "failed")) {
      out.unshift({
        id: 100000 + k++,
        host_id: h.detail.id,
        host: h.detail.hostname,
        service: s.name,
        service_type: s.type,
        kind: s.status_text === "Does not exist" ? "nonexist" : "status",
        kind_label: s.status_text,
        state: "failed",
        action: "alert",
        message: s.type === "program" ? `status failed (1) -- ${String(s.data.output ?? "").split("\n")[0]}` : "process is not running",
        created_at: s.state_since ?? t - 600,
        source: "monit",
        acked_by: null,
        acked_at: null,
      });
    }
    if (!h.detail.online) {
      out.unshift({
        id: 100000 + k++,
        host_id: h.detail.id,
        host: h.detail.hostname,
        service: null,
        service_type: null,
        kind: "heartbeat",
        kind_label: "No report from Monit",
        state: "failed",
        action: "alert",
        message: "No report from Monit for 3 poll cycles (180s)",
        created_at: h.detail.last_seen + 180,
        source: "monarch",
        acked_by: null,
        acked_at: null,
      });
    }
  }
  return out.sort((a, b) => b.created_at - a.created_at);
}

// ------------------------------------------------------------------ metrics

function metricBase(svc: Service | undefined, metric: string, host: HostDetail): [number, number, number] {
  // [base, amplitude, max]
  const d = svc?.data ?? {};
  switch (metric) {
    case "cpu":
      return svc?.type === "system" ? [host.system.cpu ?? 5, (host.system.cpu ?? 5) * 0.6 + 2, 100] : [d.cpu?.percent ?? 1, (d.cpu?.percent ?? 1) * 0.8 + 0.4, 100];
    case "cpu_user":
      return [host.system.cpu_user ?? 3, (host.system.cpu_user ?? 3) * 0.6 + 1, 100];
    case "cpu_system":
      return [host.system.cpu_system ?? 1, (host.system.cpu_system ?? 1) * 0.5 + 0.5, 100];
    case "cpu_wait":
      return [host.system.cpu_wait ?? 0.3, 0.6, 100];
    case "cpu_nice":
    case "cpu_steal":
      return [0.05, 0.08, 100];
    case "cpu_total":
      return [(d.cpu?.percent_total ?? 1) * 1, 1, 100];
    case "mem_percent":
      return svc?.type === "system" ? [host.system.mem_percent ?? 30, 2.5, 100] : [d.memory?.percent ?? 1, 0.3, 100];
    case "mem_kb":
      return svc?.type === "system" ? [host.system.mem_kb ?? 1e6, (host.system.mem_kb ?? 1e6) * 0.04, Infinity] : [d.memory?.kb ?? 1e5, (d.memory?.kb ?? 1e5) * 0.08, Infinity];
    case "mem_kb_total":
      return [d.memory?.kb_total ?? 1e5, (d.memory?.kb_total ?? 1e5) * 0.08, Infinity];
    case "swap_percent":
      return [host.system.swap_percent ?? 0, (host.system.swap_percent ?? 0) * 0.1, 100];
    case "swap_kb":
      return [host.system.swap_kb ?? 0, (host.system.swap_kb ?? 0) * 0.1, Infinity];
    case "load1":
      return [host.system.load?.[0] ?? 0.5, (host.system.load?.[0] ?? 0.5) * 0.7, Infinity];
    case "load5":
      return [host.system.load?.[1] ?? 0.5, (host.system.load?.[1] ?? 0.5) * 0.45, Infinity];
    case "load15":
      return [host.system.load?.[2] ?? 0.5, (host.system.load?.[2] ?? 0.5) * 0.25, Infinity];
    case "threads":
      return [d.threads ?? 4, 2, Infinity];
    case "children":
      return [d.children ?? 1, 1, Infinity];
    case "fd_open":
      return [d.fd?.open ?? 30, 10, Infinity];
    case "read_bps":
      return [d.io?.read_bps ?? 1e5, (d.io?.read_bps ?? 1e5) * 1.2, Infinity];
    case "write_bps":
      return [d.io?.write_bps ?? 1e5, (d.io?.write_bps ?? 1e5) * 1.2, Infinity];
    case "port_ms":
      return [d.ports?.[0]?.response_ms ?? 5, (d.ports?.[0]?.response_ms ?? 5) * 0.5, Infinity];
    case "icmp_ms":
      return [d.icmp?.[0]?.response_ms ?? 1, 0.4, Infinity];
    case "space_percent":
      return [d.space?.percent ?? 50, 0.4, 100];
    case "space_used_mb":
      return [d.space?.used_mb ?? 1000, (d.space?.used_mb ?? 1000) * 0.01, Infinity];
    case "inode_percent":
      return [d.inodes?.percent ?? 1, 0.05, 100];
    case "rx_bps":
      return [d.download?.bytes ?? 1e6, (d.download?.bytes ?? 1e6) * 0.9, Infinity];
    case "tx_bps":
      return [d.upload?.bytes ?? 1e6, (d.upload?.bytes ?? 1e6) * 0.9, Infinity];
    case "rx_pps":
      return [d.download?.packets ?? 1000, (d.download?.packets ?? 1000) * 0.8, Infinity];
    case "tx_pps":
      return [d.upload?.packets ?? 1000, (d.upload?.packets ?? 1000) * 0.8, Infinity];
    case "rx_errors":
    case "tx_errors":
      return [0, 0.2, Infinity];
    case "size":
      return [d.size ?? 1000, 0, Infinity];
    case "exit_status":
      return [svc?.state === "failed" ? 1 : 0, 0, 1];
    default:
      return [10, 3, Infinity];
  }
}

function series(host: HostDetail, service: string, metric: string, from: number, to: number) {
  const svc = host.services.find((s) => s.name === service);
  const span = to - from;
  const resolution = span <= 6 * 3600 ? 0 : span <= 7 * 86400 ? 300 : 3600;
  const step = resolution || Math.max(host.poll, 30);
  const [base, amp, max] = metricBase(svc, metric, host);
  const seed = hash(host.hostname + service + metric);
  const phase = (seed % 1000) / 160;
  const pts: MetricPoint[] = [];
  const offline = !host.online;
  const start = Math.ceil(from / step) * step;
  for (let ts = start; ts <= to; ts += step) {
    if (offline && ts > host.last_seen) break;
    const r = rng(seed ^ Math.floor(ts / step));
    const daily = Math.sin((ts / 86400) * Math.PI * 2 + phase) * amp * 0.6;
    const wobble = Math.sin(ts / 2400 + phase) * amp * 0.35 + Math.sin(ts / 700 + phase * 3) * amp * 0.12;
    const noise = (r() - 0.5) * amp * 0.22;
    const spike = r() < 0.006 ? amp * 1.6 * r() : 0;
    let v = base + daily + wobble + noise + spike;
    if (metric === "exit_status") v = base;
    v = Math.max(0, Math.min(max, v));
    const spread = resolution ? amp * 0.5 * r() : 0;
    pts.push([ts, r2(v, 3), resolution ? Math.max(0, r2(v - spread, 3)) : null, resolution ? Math.min(max, r2(v + spread, 3)) : null]);
  }
  return { resolution, points: pts };
}

// ------------------------------------------------------------------ transport

export function createMock(): MockTransport {
  const fleet = buildFleet();
  let events = buildEvents(fleet);
  let users: User[] = [
    { id: 1, username: "pschmitt", role: "admin", created_at: now() - 86400 * 400, last_login: now() - 120 },
    { id: 2, username: "anika", role: "operator", created_at: now() - 86400 * 120, last_login: now() - 86400 * 2 },
    { id: 3, username: "homeassistant", role: "viewer", created_at: now() - 86400 * 90, last_login: now() - 60 },
    { id: 4, username: "monit", role: "collector", created_at: now() - 86400 * 400, last_login: null },
  ];
  let channels: Channel[] = [
    {
      id: 1,
      name: "Signal via ntfy",
      kind: "ntfy",
      enabled: true,
      config: { url: "https://ntfy.brkn.lol/monarch", token: "********" },
      filter: { hosts: null, services: null, states: ["failed", "succeeded"], include_heartbeat: true },
      last_status: "ok",
      last_sent_at: now() - 3600,
      created_at: now() - 86400 * 30,
    },
    {
      id: 2,
      name: "Ops mailbox",
      kind: "email",
      enabled: false,
      config: { smtp_url: "********", from: "monarch@brkn.lol", to: "ops@brkn.lol" },
      filter: { hosts: "^(rofl|oci)-", services: null, states: ["failed"], include_heartbeat: true },
      last_status: null,
      last_sent_at: null,
      created_at: now() - 86400 * 12,
    },
    {
      id: 3,
      name: "n8n webhook",
      kind: "webhook",
      enabled: true,
      config: { url: "https://n8n.brkn.lol/webhook/monarch", method: "POST", headers: "X-Token: ********" },
      filter: { hosts: null, services: "backup|reboot", states: ["failed", "succeeded", "changed"], include_heartbeat: false },
      last_status: "HTTP 500: workflow inactive",
      last_sent_at: now() - 86400,
      created_at: now() - 86400 * 4,
    },
  ];
  let settings: Settings = {
    public_url: "https://monarch.brkn.lol",
    retention: { raw_hours: 48, rollup_5m_days: 30, rollup_1h_days: 400, events_days: 90 },
    heartbeat_grace: 3,
    collector_url: "https://monarch.brkn.lol/collector",
  };
  let me: User | null = users[0];
  const setupRequired = new URLSearchParams(location.search).has("setup");
  if (setupRequired || new URLSearchParams(location.search).has("loggedout")) me = null;

  const host = (id: number) => {
    const h = fleet.find((x) => x.summary.id === id);
    if (!h) throw Object.assign(new Error("host not found"), { status: 404 });
    return h;
  };
  const delay = <T>(v: T, ms = 120 + Math.random() * 180) => new Promise<T>((r) => setTimeout(() => r(structuredClone(v)), ms));

  const overview = (): Overview => {
    const hosts = fleet.map((h) => h.summary);
    const svcs = fleet.flatMap((h) => h.detail.services.map((s) => ({ s, h: h.detail })));
    const c = { total: 0, ok: 0, failed: 0, unmonitored: 0, pending: 0 };
    for (const h of hosts) for (const k of Object.keys(c) as (keyof typeof c)[]) c[k] += h.services[k];
    const procs = svcs.filter((x) => x.s.type === "process" && x.s.data.cpu);
    const t = now();
    const activity = Array.from({ length: 24 }, (_, i) => {
      const ts = Math.floor(t / 3600) * 3600 - (23 - i) * 3600;
      const inb = events.filter((e) => e.created_at >= ts && e.created_at < ts + 3600);
      return { ts, ok: inb.filter((e) => e.state !== "failed").length, failed: inb.filter((e) => e.state === "failed").length };
    });
    return {
      hosts: {
        total: hosts.length,
        online: hosts.filter((h) => h.online).length,
        offline: hosts.filter((h) => !h.online).length,
        degraded: hosts.filter((h) => h.state === "degraded").length,
      },
      services: c,
      events_24h: events.filter((e) => e.created_at > t - 86400).length,
      failing: svcs
        .filter((x) => x.s.state === "failed")
        .map((x) => ({ host_id: x.h.id, host: x.h.hostname, service: x.s.name, type: x.s.type, status_text: x.s.status_text, since: x.s.state_since })),
      top_cpu: [...procs]
        .sort((a, b) => b.s.data.cpu.percent - a.s.data.cpu.percent)
        .slice(0, 8)
        .map((x) => ({ host_id: x.h.id, host: x.h.hostname, service: x.s.name, cpu: x.s.data.cpu.percent, mem_kb: x.s.data.memory.kb, mem_percent: x.s.data.memory.percent })),
      top_mem: [...procs]
        .sort((a, b) => b.s.data.memory.kb - a.s.data.memory.kb)
        .slice(0, 8)
        .map((x) => ({ host_id: x.h.id, host: x.h.hostname, service: x.s.name, cpu: x.s.data.cpu.percent, mem_kb: x.s.data.memory.kb, mem_percent: x.s.data.memory.percent })),
      filesystems: svcs
        .filter((x) => x.s.type === "filesystem")
        .sort((a, b) => b.s.data.space.percent - a.s.data.space.percent)
        .slice(0, 8)
        .map((x) => ({ host_id: x.h.id, host: x.h.hostname, service: x.s.name, percent: x.s.data.space.percent, used_mb: x.s.data.space.used_mb, total_mb: x.s.data.space.total_mb })),
      recent_events: events.slice(0, 15),
      activity,
    };
  };

  async function request(method: string, path: string, body?: any): Promise<unknown> {
    const url = new URL(path, location.origin);
    const p = url.pathname;
    const q = url.searchParams;
    let m: RegExpMatchArray | null;

    if (p === "/api/auth/me") return delay({ user: me, setup_required: setupRequired && !me, version: "0.1.0-mock" }, 60);
    if (p === "/api/auth/login" || p === "/api/auth/setup") {
      if (!body?.username || !body?.password) throw new Error("username and password required");
      if (p === "/api/auth/login" && body.password.length < 3) throw new Error("invalid username or password");
      me = users.find((u) => u.username === body.username) ?? { ...users[0], username: body.username };
      return delay(me, 400);
    }
    if (p === "/api/auth/logout") {
      me = null;
      return delay(undefined);
    }
    if (!me) throw Object.assign(new Error("unauthorized"), { status: 401 });

    if (p === "/api/overview") return delay(overview());
    if (p === "/api/hosts") return delay(fleet.map((h) => h.summary));
    if ((m = p.match(/^\/api\/hosts\/(\d+)$/))) {
      const h = host(+m[1]);
      if (method === "GET") return delay(h.detail);
      if (method === "PATCH") {
        for (const [k, v] of Object.entries(body ?? {})) {
          if (k === "override_password") h.detail.has_override_password = !!v;
          else (h.detail as any)[k] = v === "" ? null : v;
          if (k in h.summary) (h.summary as any)[k] = v === "" ? null : v;
        }
        return delay(h.detail);
      }
      if (method === "DELETE") {
        fleet.splice(fleet.indexOf(h), 1);
        return delay(undefined);
      }
    }
    if ((m = p.match(/^\/api\/hosts\/(\d+)\/test$/))) {
      const h = host(+m[1]);
      return delay(
        h.detail.monit_url
          ? { ok: true, message: `Monit 5.35.2 responded at ${h.detail.monit_url}`, latency_ms: 12.4 }
          : { ok: false, message: "No monit httpd address known — set an override URL", latency_ms: null },
        700,
      );
    }
    if ((m = p.match(/^\/api\/hosts\/(\d+)\/services\/([^/]+)$/))) {
      const h = host(+m[1]);
      const name = decodeURIComponent(m[2]);
      const s = h.detail.services.find((x) => x.name === name);
      if (!s) throw new Error("service not found");
      return delay({ ...s, host: h.summary, recent_events: events.filter((e) => e.host_id === h.summary.id && e.service === name).slice(0, 20) });
    }
    if ((m = p.match(/^\/api\/hosts\/(\d+)\/services\/([^/]+)\/action$/)) || (m = p.match(/^\/api\/hosts\/(\d+)\/action$/))) {
      const h = host(+m[1]);
      if (!h.detail.can_act) throw new Error("monit httpd unreachable: connection refused");
      return delay({ ok: true }, 600);
    }
    if (p === "/api/metrics") {
      const h = host(+q.get("host")!);
      const to = +(q.get("to") ?? now());
      const from = +(q.get("from") ?? to - 6 * 3600);
      const metrics = (q.get("metrics") ?? "").split(",").filter(Boolean);
      let resolution = 0;
      const out = metrics.map((metric) => {
        const s = series(h.detail, q.get("service") ?? "", metric, from, to);
        resolution = s.resolution;
        return { metric, points: s.points };
      });
      return delay({ resolution, series: out });
    }
    if (p === "/api/events") {
      let list = events;
      if (q.get("host")) list = list.filter((e) => e.host_id === +q.get("host")!);
      if (q.get("service")) list = list.filter((e) => e.service === q.get("service"));
      if (q.get("state")) list = list.filter((e) => e.state === q.get("state"));
      if (q.get("unacked")) list = list.filter((e) => !e.acked_by);
      if (q.get("q")) {
        const needle = q.get("q")!.toLowerCase();
        list = list.filter((e) => `${e.host} ${e.service} ${e.message} ${e.kind_label}`.toLowerCase().includes(needle));
      }
      const limit = +(q.get("limit") ?? 100);
      // ids are not monotonic in the mock; paginate by position instead
      if (q.get("before")) {
        const idx = list.findIndex((e) => e.id === +q.get("before")!);
        list = idx >= 0 ? list.slice(idx + 1) : [];
      }
      return delay({ events: list.slice(0, limit), has_more: list.length > limit });
    }
    if ((m = p.match(/^\/api\/events\/(\d+)\/ack$/))) {
      const e = events.find((x) => x.id === +m![1])!;
      e.acked_by = me.username;
      e.acked_at = now();
      return delay(e);
    }
    if (p === "/api/events/ack") {
      for (const e of events) if (body.ids.includes(e.id)) Object.assign(e, { acked_by: me.username, acked_at: now() });
      return delay(undefined);
    }
    if (p === "/api/users") {
      if (method === "POST") {
        const u: User = { id: Math.max(...users.map((x) => x.id)) + 1, username: body.username, role: body.role, created_at: now(), last_login: null };
        users = [...users, u];
        return delay(u);
      }
      return delay(users);
    }
    if ((m = p.match(/^\/api\/users\/(\d+|me)$/))) {
      const id = m[1] === "me" ? me.id : +m[1];
      if (method === "DELETE") {
        users = users.filter((u) => u.id !== id);
        return delay(undefined);
      }
      const u = users.find((x) => x.id === id)!;
      if (body.role) u.role = body.role;
      return delay(u);
    }
    if (p === "/api/channels") {
      if (method === "POST") {
        const c: Channel = {
          id: Math.max(0, ...channels.map((x) => x.id)) + 1,
          name: body.name,
          kind: body.kind,
          enabled: body.enabled ?? true,
          config: body.config ?? {},
          filter: body.filter ?? { hosts: null, services: null, states: ["failed", "succeeded"], include_heartbeat: true },
          last_status: null,
          last_sent_at: null,
          created_at: now(),
        };
        channels = [...channels, c];
        return delay(c);
      }
      return delay(channels);
    }
    if ((m = p.match(/^\/api\/channels\/(\d+)$/))) {
      const id = +m[1];
      if (method === "DELETE") {
        channels = channels.filter((c) => c.id !== id);
        return delay(undefined);
      }
      const c = channels.find((x) => x.id === id)!;
      Object.assign(c, body);
      return delay(c);
    }
    if ((m = p.match(/^\/api\/channels\/(\d+)\/test$/))) {
      const c = channels.find((x) => x.id === +m![1])!;
      return delay(c.kind === "webhook" ? { ok: false, message: "HTTP 500: workflow inactive" } : { ok: true, message: "Test notification delivered" }, 800);
    }
    if (p === "/api/settings") {
      if (method === "PATCH") {
        settings = { ...settings, ...body, retention: { ...settings.retention, ...(body.retention ?? {}) } };
        settings.collector_url = settings.public_url.replace(/\/$/, "") + "/collector";
      }
      return delay(settings);
    }
    throw new Error(`mock: no handler for ${method} ${p}`);
  }

  function stream(onMessage: (m: StreamMessage) => void): () => void {
    const timer = setInterval(() => {
      const t = now();
      // jitter one random online host
      const online = fleet.filter((h) => h.summary.online);
      const h = online[Math.floor(Math.random() * online.length)];
      const s = h.summary;
      const jitter = (v: number | null, a: number) => (v === null ? null : r2(Math.max(0, Math.min(100, v + (Math.random() - 0.5) * a))));
      s.last_seen = t;
      s.system.cpu = jitter(s.system.cpu, 4);
      s.system.mem_percent = jitter(s.system.mem_percent, 0.6);
      s.sparkline.cpu = [...s.sparkline.cpu.slice(1), s.system.cpu ?? 0];
      s.sparkline.mem = [...s.sparkline.mem.slice(1), s.system.mem_percent ?? 0];
      Object.assign(h.detail, { last_seen: s.last_seen, system: s.system, sparkline: s.sparkline });
      onMessage({ type: "host", host: structuredClone(s) });
      if (Math.random() < 0.12) {
        const svc = h.detail.services[Math.floor(Math.random() * h.detail.services.length)];
        const ev: MonarchEvent = {
          id: Math.max(...events.map((e) => e.id)) + 1,
          host_id: s.id,
          host: s.hostname,
          service: svc.name,
          service_type: svc.type,
          kind: "resource",
          kind_label: "Resource limit matched",
          state: Math.random() < 0.5 ? "failed" : "succeeded",
          action: "alert",
          message: `cpu usage of ${r2(80 + Math.random() * 15)}% matches resource limit [cpu usage > 80.0%]`,
          created_at: t,
          source: "monit",
          acked_by: null,
          acked_at: null,
        };
        if (ev.state === "succeeded") ev.message = `cpu usage check succeeded [current cpu usage = ${r2(Math.random() * 20)}%]`;
        events = [ev, ...events];
        onMessage({ type: "event", event: ev });
      }
    }, 2500);
    return () => clearInterval(timer);
  }

  return { request: request as MockTransport["request"], stream };
}
