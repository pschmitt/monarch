import type {
  Channel,
  EventsResponse,
  HostDetail,
  HostSummary,
  Me,
  MetricsResponse,
  MonarchEvent,
  Overview,
  Role,
  ServiceAction,
  ServiceDetail,
  Settings,
  StreamMessage,
  Target,
  TargetInput,
  TargetTestResult,
  User,
} from "./types";

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

type Method = "GET" | "POST" | "PATCH" | "DELETE";

/** Transport used in mock mode; see lib/mock. */
export interface MockTransport {
  request(method: Method, path: string, body?: unknown): Promise<unknown>;
  stream(onMessage: (m: StreamMessage) => void): () => void;
}

let mock: Promise<MockTransport> | null = null;
if (import.meta.env.VITE_MOCK) {
  mock = import("./mock").then((m) => m.createMock());
}
export const isMock = !!import.meta.env.VITE_MOCK;

let unauthorizedHandler: (() => void) | null = null;
export function onUnauthorized(fn: () => void) {
  unauthorizedHandler = fn;
}

async function request<T>(method: Method, path: string, body?: unknown): Promise<T> {
  if (mock) {
    const m = await mock;
    return (await m.request(method, path, body)) as T;
  }
  const res = await fetch(path, {
    method,
    credentials: "same-origin",
    headers: body !== undefined ? { "Content-Type": "application/json", Accept: "application/json" } : { Accept: "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  let data: any = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = null;
  }
  if (!res.ok) {
    if (res.status === 401 && !path.startsWith("/api/auth/")) unauthorizedHandler?.();
    throw new ApiError(res.status, (data && data.error) || res.statusText || `HTTP ${res.status}`);
  }
  return data as T;
}

function qs(params: Record<string, string | number | boolean | null | undefined>): string {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "" || v === false) continue;
    u.set(k, v === true ? "1" : String(v));
  }
  const s = u.toString();
  return s ? `?${s}` : "";
}

const enc = encodeURIComponent;

export const api = {
  me: () => request<Me>("GET", "/api/auth/me"),
  setup: (username: string, password: string) => request<User>("POST", "/api/auth/setup", { username, password }),
  login: (username: string, password: string) => request<User>("POST", "/api/auth/login", { username, password }),
  logout: () => request<void>("POST", "/api/auth/logout"),

  overview: () => request<Overview>("GET", "/api/overview"),

  hosts: () => request<HostSummary[]>("GET", "/api/hosts"),
  host: (id: number) => request<HostDetail>("GET", `/api/hosts/${id}`),
  updateHost: (id: number, patch: Record<string, unknown>) => request<HostDetail>("PATCH", `/api/hosts/${id}`, patch),
  deleteHost: (id: number) => request<void>("DELETE", `/api/hosts/${id}`),
  testHost: (id: number) =>
    request<{ ok: boolean; message: string; latency_ms: number | null }>("POST", `/api/hosts/${id}/test`),

  service: (hostId: number, name: string) =>
    request<ServiceDetail>("GET", `/api/hosts/${hostId}/services/${enc(name)}`),
  serviceAction: (hostId: number, name: string, action: ServiceAction) =>
    request<{ ok: boolean }>("POST", `/api/hosts/${hostId}/services/${enc(name)}/action`, { action }),
  bulkAction: (hostId: number, action: ServiceAction, services: string[]) =>
    request<{ ok: boolean }>("POST", `/api/hosts/${hostId}/action`, { action, services }),

  metrics: (p: { host: number; service: string; metrics: string[]; from?: number; to?: number }) =>
    request<MetricsResponse>(
      "GET",
      `/api/metrics${qs({ host: p.host, service: p.service, metrics: p.metrics.join(","), from: p.from, to: p.to })}`,
    ),

  events: (p: {
    host?: number | null;
    service?: string | null;
    state?: string | null;
    q?: string | null;
    unacked?: boolean;
    limit?: number;
    before?: number | null;
  }) => request<EventsResponse>("GET", `/api/events${qs(p as any)}`),
  ackEvent: (id: number) => request<MonarchEvent>("POST", `/api/events/${id}/ack`),
  ackEvents: (ids: number[]) => request<void>("POST", "/api/events/ack", { ids }),

  users: () => request<User[]>("GET", "/api/users"),
  createUser: (u: { username: string; password: string; role: Role }) => request<User>("POST", "/api/users", u),
  updateUser: (id: number | "me", patch: Record<string, unknown>) => request<User>("PATCH", `/api/users/${id}`, patch),
  deleteUser: (id: number) => request<void>("DELETE", `/api/users/${id}`),

  channels: () => request<Channel[]>("GET", "/api/channels"),
  createChannel: (c: Partial<Channel>) => request<Channel>("POST", "/api/channels", c),
  updateChannel: (id: number, c: Partial<Channel>) => request<Channel>("PATCH", `/api/channels/${id}`, c),
  deleteChannel: (id: number) => request<void>("DELETE", `/api/channels/${id}`),
  testChannel: (id: number) => request<{ ok: boolean; message: string }>("POST", `/api/channels/${id}/test`),

  targets: () => request<Target[]>("GET", "/api/targets"),
  createTarget: (t: TargetInput) => request<Target>("POST", "/api/targets", t),
  updateTarget: (id: number, t: Partial<TargetInput>) => request<Target>("PATCH", `/api/targets/${id}`, t),
  deleteTarget: (id: number) => request<void>("DELETE", `/api/targets/${id}`),
  testTarget: (t: TargetInput | { id: number }) => request<TargetTestResult>("POST", "/api/targets/test", t),
  pollTarget: (id: number) => request<Target>("POST", `/api/targets/${id}/poll`),

  settings: () => request<Settings>("GET", "/api/settings"),
  updateSettings: (s: Partial<Settings>) => request<Settings>("PATCH", "/api/settings", s),
};

export type StreamStatus = "connecting" | "live" | "down";

/** Subscribe to /api/stream with automatic reconnect. Returns an unsubscribe fn. */
export function openStream(onMessage: (m: StreamMessage) => void, onStatus: (s: StreamStatus) => void): () => void {
  if (mock) {
    let stop: (() => void) | null = null;
    let cancelled = false;
    onStatus("connecting");
    mock.then((m) => {
      if (cancelled) return;
      onStatus("live");
      stop = m.stream(onMessage);
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }
  let es: EventSource | null = null;
  let retry = 1000;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;
  const connect = () => {
    if (closed) return;
    onStatus("connecting");
    es = new EventSource("/api/stream", { withCredentials: true });
    es.onopen = () => {
      retry = 1000;
      onStatus("live");
    };
    es.onmessage = (ev) => {
      try {
        onMessage(JSON.parse(ev.data));
      } catch {
        /* ignore malformed frames */
      }
    };
    es.onerror = () => {
      es?.close();
      onStatus("down");
      if (closed) return;
      timer = setTimeout(connect, retry);
      retry = Math.min(retry * 2, 30000);
    };
  };
  connect();
  return () => {
    closed = true;
    if (timer) clearTimeout(timer);
    es?.close();
  };
}
