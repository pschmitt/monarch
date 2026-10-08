// Minimal history-API router.

export type RouteName =
  | "overview"
  | "hosts"
  | "services"
  | "host"
  | "service"
  | "events"
  | "settings"
  | "login"
  | "setup"
  | "notfound";

export interface Route {
  name: RouteName;
  params: Record<string, string>;
  query: URLSearchParams;
  path: string;
}

const patterns: [RouteName, RegExp, string[]][] = [
  ["overview", /^\/$/, []],
  ["hosts", /^\/hosts\/?$/, []],
  ["services", /^\/services\/?$/, []],
  ["service", /^\/hosts\/(\d+)\/services\/([^/]+)\/?$/, ["id", "name"]],
  ["host", /^\/hosts\/(\d+)(?:\/([a-z]+))?\/?$/, ["id", "tab"]],
  ["events", /^\/events\/?$/, []],
  ["settings", /^\/settings(?:\/([a-z]+))?\/?$/, ["section"]],
  ["login", /^\/login\/?$/, []],
  ["setup", /^\/setup\/?$/, []],
];

function parse(): Route {
  const path = window.location.pathname;
  const query = new URLSearchParams(window.location.search);
  for (const [name, re, keys] of patterns) {
    const m = path.match(re);
    if (m) {
      const params: Record<string, string> = {};
      keys.forEach((k, i) => {
        if (m[i + 1] !== undefined) params[k] = decodeURIComponent(m[i + 1]);
      });
      return { name, params, query, path };
    }
  }
  return { name: "notfound", params: {}, query, path };
}

class Router {
  route = $state<Route>(parse());

  constructor() {
    window.addEventListener("popstate", () => {
      this.route = parse();
    });
    // Intercept same-origin <a> clicks so links work without full reloads.
    document.addEventListener("click", (e) => {
      if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      const a = (e.target as HTMLElement | null)?.closest?.("a");
      if (!a || a.target || a.hasAttribute("download") || a.dataset.external !== undefined) return;
      const href = a.getAttribute("href");
      if (!href || !href.startsWith("/") || href.startsWith("//")) return;
      e.preventDefault();
      this.go(href);
    });
  }

  go(href: string, replace = false) {
    const cur = window.location.pathname + window.location.search;
    if (href === cur) return;
    if (replace) history.replaceState(null, "", href);
    else history.pushState(null, "", href);
    this.route = parse();
    if (!replace) window.scrollTo({ top: 0 });
  }

  /** Update query params without adding history entries. */
  setQuery(params: Record<string, string | null | undefined>) {
    const q = new URLSearchParams(window.location.search);
    for (const [k, v] of Object.entries(params)) {
      if (v === null || v === undefined || v === "") q.delete(k);
      else q.set(k, v);
    }
    const s = q.toString();
    history.replaceState(null, "", window.location.pathname + (s ? `?${s}` : ""));
    this.route = parse();
  }
}

export const router = new Router();

export const hostHref = (id: number, tab?: string) => `/hosts/${id}${tab ? `/${tab}` : ""}`;
export const serviceHref = (hostId: number, name: string) => `/hosts/${hostId}/services/${encodeURIComponent(name)}`;
