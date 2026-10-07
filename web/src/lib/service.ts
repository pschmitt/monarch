import type { Service } from "./types";
import { bits, bytes, duration, kb, mb, ms, num, pct, rate } from "./format";

export interface Metric {
  label: string;
  value: string;
  /** optional 0-100 gauge value */
  percent?: number | null;
}

/** Up to three key metrics for compact service rows. */
export function keyMetrics(s: Service): Metric[] {
  const d = s.data ?? {};
  switch (s.type) {
    case "system":
      return [
        { label: "CPU", value: pct(d.cpu?.total), percent: d.cpu?.total },
        { label: "Mem", value: pct(d.memory?.percent), percent: d.memory?.percent },
        { label: "Load 1m", value: d.load ? d.load[0].toFixed(2) : "—" },
      ];
    case "process":
      if (d.pid === undefined || d.pid === null) return [];
      return [
        { label: "CPU", value: pct(d.cpu?.percent), percent: d.cpu?.percent },
        { label: "Mem", value: kb(d.memory?.kb) },
        { label: "PID", value: String(d.pid) },
      ];
    case "filesystem":
      return [
        { label: "Used", value: pct(d.space?.percent), percent: d.space?.percent },
        { label: "Free", value: d.space ? mb((d.space.total_mb ?? 0) - (d.space.used_mb ?? 0)) : "—" },
      ];
    case "net":
      return [
        { label: "↓", value: rate(d.download?.bytes) },
        { label: "↑", value: rate(d.upload?.bytes) },
        { label: "Link", value: d.link?.state === 1 ? (d.link.speed > 0 ? bits(d.link.speed) : "up") : d.link?.state === 0 ? "down" : "n/a" },
      ];
    case "host": {
      const out: Metric[] = [];
      const icmp = d.icmp?.[0];
      if (icmp) out.push({ label: "Ping", value: icmp.response_ms === null ? "failed" : ms(icmp.response_ms) });
      const port = d.ports?.[0];
      if (port) out.push({ label: `:${port.port}`, value: port.response_ms === null ? "failed" : ms(port.response_ms) });
      if (port?.cert_valid_days !== null && port?.cert_valid_days !== undefined) out.push({ label: "Cert", value: `${port.cert_valid_days}d` });
      return out;
    }
    case "program":
      return [
        { label: "Exit", value: d.exit_status === undefined || d.exit_status === null ? "—" : String(d.exit_status) },
        { label: "Ran", value: d.started ? `${duration(Date.now() / 1000 - d.started, 1)} ago` : "—" },
      ];
    case "file":
      return [
        { label: "Size", value: bytes(d.size) },
        { label: "Mode", value: d.mode ?? "—" },
      ];
    case "directory":
    case "fifo":
      return [{ label: "Mode", value: d.mode ?? "—" }];
  }
  return [];
}

export interface ChartDef {
  title: string;
  metrics: { key: string; label: string; color: string; fill?: boolean; dashed?: boolean }[];
  format: (v: number) => string;
  stacked?: boolean;
  yMax?: number | null;
}

const P = (v: number) => `${v.toFixed(v < 10 ? 1 : 0)}%`;
const C = {
  a: "var(--accent)",
  b: "var(--accent-2)",
  ok: "var(--ok)",
  warn: "var(--warn)",
  bad: "var(--bad)",
  info: "var(--info)",
  muted: "var(--muted)",
};

export function chartsFor(s: Service): ChartDef[] {
  switch (s.type) {
    case "system":
      return systemCharts();
    case "process":
      return [
        { title: "CPU usage", metrics: [{ key: "cpu", label: "Process", color: C.a }, { key: "cpu_total", label: "Incl. children", color: C.b, fill: false, dashed: true }], format: P },
        { title: "Memory", metrics: [{ key: "mem_kb", label: "Resident", color: C.b }, { key: "mem_kb_total", label: "Incl. children", color: C.a, fill: false, dashed: true }], format: (v) => kb(v) },
        { title: "Disk I/O", metrics: [{ key: "read_bps", label: "Read", color: C.ok }, { key: "write_bps", label: "Write", color: C.warn }], format: (v) => rate(v) },
        { title: "Threads & file descriptors", metrics: [{ key: "threads", label: "Threads", color: C.a }, { key: "fd_open", label: "Open FDs", color: C.info, fill: false }, { key: "children", label: "Children", color: C.muted, fill: false }], format: (v) => num(v) },
        ...(s.data?.ports?.length ? [{ title: "Port response time", metrics: [{ key: "port_ms", label: "Response", color: C.b }], format: (v: number) => ms(v) }] : []),
      ];
    case "filesystem":
      return [
        { title: "Space used", metrics: [{ key: "space_percent", label: "Space", color: C.a }, { key: "inode_percent", label: "Inodes", color: C.b, fill: false, dashed: true }], format: P, yMax: 100 },
        { title: "Disk I/O", metrics: [{ key: "read_bps", label: "Read", color: C.ok }, { key: "write_bps", label: "Write", color: C.warn }], format: (v) => rate(v) },
      ];
    case "net":
      return [
        { title: "Throughput", metrics: [{ key: "rx_bps", label: "Download", color: C.b }, { key: "tx_bps", label: "Upload", color: C.a }], format: (v) => rate(v) },
        { title: "Packets", metrics: [{ key: "rx_pps", label: "In", color: C.b }, { key: "tx_pps", label: "Out", color: C.a }], format: (v) => `${num(v)}/s` },
        { title: "Errors", metrics: [{ key: "rx_errors", label: "In", color: C.bad }, { key: "tx_errors", label: "Out", color: C.warn }], format: (v) => `${num(v, 1)}/s` },
      ];
    case "host":
      return [
        {
          title: "Response time",
          metrics: [
            ...(s.data?.icmp?.length ? [{ key: "icmp_ms", label: "Ping", color: C.b }] : []),
            ...(s.data?.ports?.length ? [{ key: "port_ms", label: "Port", color: C.a }] : []),
          ],
          format: (v) => ms(v),
        },
      ];
    case "program":
      return [{ title: "Exit status", metrics: [{ key: "exit_status", label: "Exit", color: C.warn }], format: (v) => (Number.isInteger(v) ? String(v) : "") }];
    case "file":
      return [{ title: "File size", metrics: [{ key: "size", label: "Size", color: C.a }], format: (v) => bytes(v) }];
  }
  return [];
}

export function systemCharts(): ChartDef[] {
  return [
    {
      title: "CPU",
      metrics: [
        { key: "cpu_user", label: "User", color: C.a },
        { key: "cpu_system", label: "System", color: C.b },
        { key: "cpu_wait", label: "I/O wait", color: C.warn },
        { key: "cpu_nice", label: "Nice", color: C.info },
        { key: "cpu_steal", label: "Steal", color: C.bad },
      ],
      format: P,
      stacked: true,
    },
    {
      title: "Memory & swap",
      metrics: [
        { key: "mem_percent", label: "Memory", color: C.b },
        { key: "swap_percent", label: "Swap", color: C.warn, fill: false },
      ],
      format: P,
      yMax: 100,
    },
    {
      title: "Load average",
      metrics: [
        { key: "load1", label: "1 min", color: C.a },
        { key: "load5", label: "5 min", color: C.b, fill: false },
        { key: "load15", label: "15 min", color: C.muted, fill: false, dashed: true },
      ],
      format: (v) => v.toFixed(2),
    },
  ];
}
