// Types mirroring docs/API.md — keep in sync with the backend contract.

export type Role = "admin" | "operator" | "viewer" | "collector";

export interface User {
  id: number;
  username: string;
  role: Role;
  created_at: number;
  last_login: number | null;
  auth_source: "local" | "oidc";
  /** Has a single sign-on identity (SSO account, or a local one linked to it). */
  sso: boolean;
}

export interface Me {
  user: User | null;
  setup_required: boolean;
  version: string;
  /** Single sign-on provider, when configured. */
  oidc: { name: string } | null;
  /** Whether username/password sign-in is offered. */
  password_login: boolean;
}

export type ServiceType =
  | "filesystem"
  | "directory"
  | "file"
  | "process"
  | "host"
  | "system"
  | "fifo"
  | "program"
  | "net";

export type ServiceState = "ok" | "failed" | "unmonitored" | "pending" | "init";
export type HostState = "ok" | "degraded" | "offline";
export type EventState = "succeeded" | "failed" | "changed" | "changed_not" | "init";

export interface ServiceCounts {
  total: number;
  ok: number;
  failed: number;
  unmonitored: number;
  pending: number;
}

export interface HostSystem {
  uptime: number | null;
  cpu: number | null;
  cpu_user: number | null;
  cpu_system: number | null;
  cpu_wait: number | null;
  mem_percent: number | null;
  mem_kb: number | null;
  swap_percent: number | null;
  swap_kb: number | null;
  load: [number, number, number] | null;
}

export interface HostSummary {
  id: number;
  monit_id: string;
  hostname: string;
  display_name: string | null;
  description: string | null;
  state: HostState;
  online: boolean;
  last_seen: number;
  first_seen: number;
  poll: number;
  monit_version: string | null;
  monit_uptime: number | null;
  os: { name: string | null; release: string | null; version: string | null; machine: string | null };
  cpu_count: number | null;
  mem_total_kb: number | null;
  swap_total_kb: number | null;
  hostgroups: string[];
  services: ServiceCounts;
  system: HostSystem;
  sparkline: { cpu: number[]; mem: number[] };
  failing: string[];
  muted_until: number | null;
  can_act: boolean;
  source: "push" | "pull";
  target_id: number | null;
}

// NOTE: API.md declares `HostDetail extends HostSummary` but redefines
// `services` as Service[]; the detail payload therefore carries the service
// list instead of the counts (counts are derived client-side).
export interface HostDetail extends Omit<HostSummary, "services"> {
  controlfile: string | null;
  incarnation: number | null;
  startdelay: number | null;
  httpd: { address: string | null; port: number | null; ssl: boolean; unixsocket: string | null } | null;
  remote_addr: string | null;
  monit_url: string | null;
  override_url: string | null;
  override_username: string | null;
  has_override_password: boolean;
  has_reported_credentials: boolean;
  tls_skip_verify: boolean;
  servicegroups: Record<string, string[]>;
  services: Service[];
}

export interface Port {
  hostname: string;
  port: number;
  protocol: string;
  type: string;
  request: string | null;
  response_ms: number | null;
  cert_valid_days: number | null;
}

export interface UnixSocket {
  path: string;
  protocol: string;
  response_ms: number | null;
}

// ServiceData is type dependent; every field is optional.
export type ServiceData = Record<string, any>;

export interface Service {
  id: number;
  host_id: number;
  name: string;
  type: ServiceType;
  type_id: number;
  state: ServiceState;
  status_text: string;
  status: number;
  monitor: number;
  monitor_mode: "active" | "passive";
  pending_action: string | null;
  every: string | null;
  collected_at: number;
  state_since: number | null;
  groups: string[];
  events: number;
  data: ServiceData;
}

export interface ServiceDetail extends Service {
  host: HostSummary;
  recent_events: MonarchEvent[];
}

export interface MonarchEvent {
  id: number;
  host_id: number | null;
  host: string | null;
  service: string | null;
  service_type: ServiceType | null;
  kind: string;
  kind_label: string;
  state: EventState;
  action: string | null;
  message: string;
  created_at: number;
  source: "monit" | "monarch";
  acked_by: string | null;
  acked_at: number | null;
}

export interface ServiceRef {
  host_id: number;
  host: string;
  service: string;
  type: ServiceType;
  status_text: string;
  since: number | null;
}

export interface ProcessRef {
  host_id: number;
  host: string;
  service: string;
  cpu: number | null;
  mem_kb: number | null;
  mem_percent: number | null;
}

export interface FilesystemRef {
  host_id: number;
  host: string;
  service: string;
  percent: number;
  used_mb: number;
  total_mb: number;
}

export interface Overview {
  hosts: { total: number; online: number; offline: number; degraded: number };
  services: ServiceCounts;
  events_24h: number;
  failing: ServiceRef[];
  top_cpu: ProcessRef[];
  top_mem: ProcessRef[];
  filesystems: FilesystemRef[];
  recent_events: MonarchEvent[];
  activity: { ts: number; ok: number; failed: number }[];
}

export type MetricPoint = [number, number, number | null, number | null];

export interface MetricsResponse {
  resolution: number;
  series: { metric: string; points: MetricPoint[] }[];
}

export interface EventsResponse {
  events: MonarchEvent[];
  has_more: boolean;
}

export type ChannelKind = "webhook" | "ntfy" | "gotify" | "slack" | "discord" | "telegram" | "email";

export interface ChannelFilter {
  hosts: string | null;
  services: string | null;
  states: EventState[];
  include_heartbeat: boolean;
}

export interface Channel {
  id: number;
  name: string;
  kind: ChannelKind;
  enabled: boolean;
  config: Record<string, string>;
  filter: ChannelFilter;
  last_status: string | null;
  last_sent_at: number | null;
  created_at: number;
}

export interface Settings {
  public_url: string;
  retention: { raw_hours: number; rollup_5m_days: number; rollup_1h_days: number; events_days: number };
  heartbeat_grace: number;
  collector_url: string;
}

export type StreamMessage =
  | { type: "host"; host: HostSummary }
  | { type: "host_removed"; id: number }
  | { type: "event"; event: MonarchEvent }
  | { type: "target"; target: Target }
  | { type: "ping"; ts: number };

export type ServiceAction = "start" | "stop" | "restart" | "monitor" | "unmonitor";

export interface Target {
  id: number;
  name: string;
  url: string;
  username: string | null;
  has_password: boolean;
  ssh: { destination: string; port: number | null } | null;
  interval: number;
  tls_skip_verify: boolean;
  enabled: boolean;
  managed: boolean;
  host_id: number | null;
  last_status: string | null;
  last_polled_at: number | null;
  created_at: number;
}

export interface TargetInput {
  name: string;
  url: string;
  username?: string | null;
  password?: string | null;
  ssh?: { destination: string; port?: number | null } | null;
  interval?: number;
  tls_skip_verify?: boolean;
  enabled?: boolean;
}

export interface TargetTestResult {
  ok: boolean;
  message: string;
  hostname: string | null;
  monit_version: string | null;
  services: number | null;
  latency_ms: number | null;
}

export interface ApiToken {
  id: number;
  name: string;
  prefix: string;
  created_at: number;
  last_used: number | null;
  expires_at: number | null;
}
