import type { EventState, HostState, ServiceState, ServiceType } from "./types";

const UNITS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

export function bytes(n: number | null | undefined, digits = 1): string {
  if (n === null || n === undefined || !isFinite(n)) return "—";
  let v = Math.abs(n);
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  const sign = n < 0 ? "-" : "";
  return `${sign}${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(digits)} ${UNITS[i]}`;
}

export const kb = (n: number | null | undefined, digits = 1) =>
  n === null || n === undefined ? "—" : bytes(n * 1024, digits);
export const mb = (n: number | null | undefined, digits = 1) =>
  n === null || n === undefined ? "—" : bytes(n * 1024 * 1024, digits);

export function rate(n: number | null | undefined): string {
  if (n === null || n === undefined || !isFinite(n)) return "—";
  return `${bytes(n)}/s`;
}

export function bits(n: number | null | undefined): string {
  if (n === null || n === undefined || n <= 0) return "—";
  const u = ["bit/s", "kbit/s", "Mbit/s", "Gbit/s", "Tbit/s"];
  let v = n;
  let i = 0;
  while (v >= 1000 && i < u.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v % 1 === 0 ? v : v.toFixed(1)} ${u[i]}`;
}

export function pct(n: number | null | undefined, digits = 1): string {
  if (n === null || n === undefined || !isFinite(n)) return "—";
  return `${n.toFixed(digits)}%`;
}

export function num(n: number | null | undefined, digits = 0): string {
  if (n === null || n === undefined || !isFinite(n)) return "—";
  return n.toLocaleString(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits });
}

export function compact(n: number | null | undefined): string {
  if (n === null || n === undefined || !isFinite(n)) return "—";
  return Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(n);
}

export function ms(n: number | null | undefined): string {
  if (n === null || n === undefined || !isFinite(n)) return "—";
  if (n < 1) return `${(n * 1000).toFixed(0)} µs`;
  if (n < 100) return `${n.toFixed(2)} ms`;
  if (n < 1000) return `${n.toFixed(0)} ms`;
  return `${(n / 1000).toFixed(2)} s`;
}

/** Seconds → "3d 4h", "5h 12m", "42s". */
export function duration(s: number | null | undefined, parts = 2): string {
  if (s === null || s === undefined || !isFinite(s)) return "—";
  s = Math.max(0, Math.floor(s));
  const units: [string, number][] = [
    ["y", 31536000],
    ["d", 86400],
    ["h", 3600],
    ["m", 60],
    ["s", 1],
  ];
  if (s === 0) return "0s";
  const out: string[] = [];
  let started = false;
  let used = 0;
  for (const [u, v] of units) {
    const q = Math.floor(s / v);
    s -= q * v;
    if (q > 0) started = true;
    if (!started) continue;
    if (q > 0) out.push(`${q}${u}`);
    if (++used >= parts) break;
  }
  return out.join(" ");
}

export function nowSec(): number {
  return Date.now() / 1000;
}

/** Unix seconds → "12s ago", "in 3m", "3d ago". */
export function ago(ts: number | null | undefined, now = nowSec()): string {
  if (!ts) return "never";
  const d = now - ts;
  if (Math.abs(d) < 5) return "just now";
  const s = duration(Math.abs(d), 1);
  return d >= 0 ? `${s} ago` : `in ${s}`;
}

export function datetime(ts: number | null | undefined): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function timeShort(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

export function toLocalInput(ts: number | null | undefined): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export const serviceTypeLabel: Record<ServiceType, string> = {
  system: "System",
  process: "Process",
  filesystem: "Filesystem",
  directory: "Directory",
  file: "File",
  fifo: "FIFO",
  host: "Remote host",
  program: "Program",
  net: "Network",
};

export const serviceTypeOrder: ServiceType[] = [
  "system",
  "process",
  "program",
  "host",
  "filesystem",
  "net",
  "file",
  "directory",
  "fifo",
];

export type Tone = "ok" | "warn" | "bad" | "muted" | "info";

export function serviceTone(s: ServiceState): Tone {
  switch (s) {
    case "ok":
      return "ok";
    case "failed":
      return "bad";
    case "pending":
    case "init":
      return "warn";
    default:
      return "muted";
  }
}

export function hostTone(s: HostState): Tone {
  return s === "ok" ? "ok" : s === "degraded" ? "warn" : "bad";
}

export function eventTone(s: EventState): Tone {
  switch (s) {
    case "succeeded":
      return "ok";
    case "failed":
      return "bad";
    case "changed":
      return "info";
    default:
      return "muted";
  }
}

export const hostStateLabel: Record<HostState, string> = {
  ok: "Healthy",
  degraded: "Degraded",
  offline: "Offline",
};

export const eventStateLabel: Record<EventState, string> = {
  succeeded: "Recovered",
  failed: "Failed",
  changed: "Changed",
  changed_not: "Not changed",
  init: "Init",
};

/** Threshold tone for a percentage gauge. */
export function usageTone(p: number | null | undefined): Tone {
  if (p === null || p === undefined) return "muted";
  if (p >= 90) return "bad";
  if (p >= 75) return "warn";
  return "ok";
}

export function hostName(h: { display_name: string | null; hostname: string }): string {
  return h.display_name || h.hostname;
}

export function osLabel(os: { name: string | null; release: string | null; machine: string | null }): string {
  return [os.name, os.release, os.machine].filter(Boolean).join(" · ") || "Unknown OS";
}

export function octalMode(m: unknown): string {
  if (m === null || m === undefined || m === "") return "—";
  const s = String(m);
  return s.padStart(4, "0");
}
