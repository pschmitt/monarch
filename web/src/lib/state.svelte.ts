import { api, openStream, type StreamStatus } from "./api";
import type { HostSummary, Me, MonarchEvent, Role, StreamMessage, Target } from "./types";
import { hostName } from "./format";

// ---------------------------------------------------------------- session

export const session = $state<{ me: Me | null; loading: boolean }>({ me: null, loading: true });

export async function refreshSession() {
  try {
    session.me = await api.me();
  } catch {
    session.me = { user: null, setup_required: false, version: "?", oidc: null, password_login: true };
  } finally {
    session.loading = false;
  }
}

const rank: Record<Role, number> = { collector: 0, viewer: 1, operator: 2, admin: 3 };
export function can(min: Role): boolean {
  const r = session.me?.user?.role;
  return !!r && rank[r] >= rank[min];
}

// ---------------------------------------------------------------- clock

export const clock = $state({ now: Date.now() / 1000 });
setInterval(() => (clock.now = Date.now() / 1000), 1000);

// ---------------------------------------------------------------- theme

export type Theme = "dark" | "light";
function initialTheme(): Theme {
  try {
    return localStorage.getItem("monarch.theme") === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}
export const theme = $state<{ value: Theme }>({ value: initialTheme() });
export function setTheme(t: Theme) {
  theme.value = t;
  const root = document.documentElement;
  root.classList.remove("dark", "light");
  root.classList.add(t);
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", t === "dark" ? "#07080c" : "#f6f7fb");
  try {
    localStorage.setItem("monarch.theme", t);
  } catch {
    /* private mode */
  }
}
export const toggleTheme = () => setTheme(theme.value === "dark" ? "light" : "dark");

// ---------------------------------------------------------------- toasts

export type ToastTone = "ok" | "bad" | "warn" | "info";
export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  body?: string;
  href?: string;
}
export const toasts = $state<Toast[]>([]);
let toastSeq = 0;
export function toast(tone: ToastTone, title: string, body?: string, href?: string, ttl = 5000) {
  const id = ++toastSeq;
  toasts.push({ id, tone, title, body, href });
  if (toasts.length > 5) toasts.splice(0, toasts.length - 5);
  setTimeout(() => dismissToast(id), ttl);
}
export function dismissToast(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}
export function toastError(e: unknown, title = "Something went wrong") {
  toast("bad", title, e instanceof Error ? e.message : String(e));
}

// ---------------------------------------------------------------- confirm dialog

export interface ConfirmRequest {
  title: string;
  body?: string;
  confirm?: string;
  danger?: boolean;
  resolve: (ok: boolean) => void;
}
export const confirmState = $state<{ req: ConfirmRequest | null }>({ req: null });
export function confirm(opts: Omit<ConfirmRequest, "resolve">): Promise<boolean> {
  return new Promise((resolve) => {
    confirmState.req = { ...opts, resolve };
  });
}

// ---------------------------------------------------------------- fleet (live)

export const fleet = $state<{
  hosts: Record<number, HostSummary>;
  loaded: boolean;
  status: StreamStatus;
  /** Most recent events received live (newest first). */
  live: MonarchEvent[];
  /** Bumped on every live message so pages can react (e.g. refetch). */
  tick: number;
}>({ hosts: {}, loaded: false, status: "connecting", live: [], tick: 0 });

export const hostList = () =>
  Object.values(fleet.hosts).sort((a, b) => hostName(a).localeCompare(hostName(b), undefined, { numeric: true }));

export async function loadHosts() {
  try {
    const list = await api.hosts();
    const map: Record<number, HostSummary> = {};
    for (const h of list) map[h.id] = h;
    fleet.hosts = map;
    fleet.loaded = true;
  } catch (e) {
    toastError(e, "Failed to load hosts");
  }
}

const eventListeners = new Set<(e: MonarchEvent) => void>();
export function onLiveEvent(fn: (e: MonarchEvent) => void): () => void {
  eventListeners.add(fn);
  return () => eventListeners.delete(fn);
}

const targetListeners = new Set<(t: Target) => void>();
/** Subscribe to live connection (pull target) updates sent after each poll. */
export function onLiveTarget(fn: (t: Target) => void): () => void {
  targetListeners.add(fn);
  return () => targetListeners.delete(fn);
}

function handle(msg: StreamMessage) {
  switch (msg.type) {
    case "host":
      fleet.hosts[msg.host.id] = msg.host;
      break;
    case "host_removed":
      delete fleet.hosts[msg.id];
      break;
    case "event": {
      fleet.live.unshift(msg.event);
      if (fleet.live.length > 50) fleet.live.length = 50;
      const ev = msg.event;
      if (ev.state === "failed") {
        toast(
          "bad",
          `${ev.host ?? "Monarch"}${ev.service ? ` · ${ev.service}` : ""}`,
          ev.message,
          ev.host_id ? `/hosts/${ev.host_id}` : undefined,
          8000,
        );
      } else if (ev.state === "succeeded" && ev.source === "monarch") {
        toast("ok", `${ev.host ?? "Monarch"}`, ev.message);
      }
      for (const fn of eventListeners) fn(ev);
      break;
    }
    case "target":
      for (const fn of targetListeners) fn(msg.target);
      return;
    case "ping":
      return;
  }
  fleet.tick++;
}

let closeStream: (() => void) | null = null;
export function startLive() {
  if (closeStream) return;
  loadHosts();
  closeStream = openStream(handle, (s) => {
    const prev = fleet.status;
    fleet.status = s;
    // Resync after a reconnect so we don't miss state changes.
    if (s === "live" && prev === "down") loadHosts();
  });
}
export function stopLive() {
  closeStream?.();
  closeStream = null;
  fleet.hosts = {};
  fleet.loaded = false;
  fleet.live = [];
}

// ---------------------------------------------------------------- ui prefs

function loadBool(key: string, def: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? def : v === "1";
  } catch {
    return def;
  }
}
export const ui = $state({
  sidebarCollapsed: loadBool("monarch.sidebar", false),
  mobileNav: false,
  palette: false,
  addHost: false,
});
export function toggleSidebar() {
  ui.sidebarCollapsed = !ui.sidebarCollapsed;
  try {
    localStorage.setItem("monarch.sidebar", ui.sidebarCollapsed ? "1" : "0");
  } catch {
    /* ignore */
  }
}
