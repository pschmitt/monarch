<script lang="ts">
  import { untrack, type Component } from "svelte";
  import {
    Activity,
    Bell,
    BellOff,
    ChevronRight,
    CornerDownLeft,
    Eye,
    EyeOff,
    LayoutDashboard,
    List,
    LogOut,
    Moon,
    Palette,
    PanelLeft,
    Play,
    Plus,
    RefreshCw,
    RotateCcw,
    Search,
    Server,
    Settings,
    Square,
    Sun,
    X,
  } from "@lucide/svelte";
  import { api } from "../api";
  import { ACTIONS, runAction } from "../actions";
  import { fuzzy } from "../fuzzy";
  import { hostName, hostTone, serviceTone } from "../format";
  import { isHostSilenced } from "../silence";
  import { router, hostHref, serviceHref } from "../router.svelte";
  import { can, clock, confirm, fleet, hostList, setThemeStyle, theme, themeStyles, toast, toastError, toggleSidebar, toggleTheme, ui } from "../state.svelte";
  import type { HostSummary, ServiceAction, ServiceListItem, ServiceType } from "../types";
  import ServiceIcon from "./ServiceIcon.svelte";
  import StatusDot from "./StatusDot.svelte";

  let { onlogout }: { onlogout: () => void } = $props();

  type Tone = "ok" | "warn" | "bad" | "muted" | "info";
  interface Item {
    id: string;
    group: string;
    label: string;
    hint?: string;
    icon?: Component<any>;
    serviceType?: ServiceType;
    tone?: Tone;
    /** Navigate or act; the palette closes first. */
    run?: () => void;
    /** Follow-up list (actions for a host or service), opened with Tab / → or Enter when there is no `run`. */
    sub?: () => Item[];
    subTitle?: string;
  }
  interface Frame {
    title: string;
    items: () => Item[];
  }

  /** Query prefixes that narrow the search to one kind of result. */
  const SCOPES = [
    { prefix: ">", label: "commands", groups: ["Pages", "Actions"] },
    { prefix: "@", label: "hosts", groups: ["Hosts"] },
    { prefix: "#", label: "services", groups: ["Services"] },
    { prefix: "!", label: "failing", groups: ["Failing"] },
  ] as const;

  const actionIcons: Record<ServiceAction, Component<any>> = {
    start: Play,
    stop: Square,
    restart: RefreshCw,
    monitor: Eye,
    unmonitor: EyeOff,
    remonitor: RotateCcw,
  };

  let q = $state("");
  let sel = $state(0);
  let input = $state<HTMLInputElement>();
  let stack = $state<Frame[]>([]);
  let services = $state<ServiceListItem[]>([]);
  let servicesLoadedAt = 0;

  async function loadServices() {
    if (Date.now() - servicesLoadedAt < 30_000) return;
    servicesLoadedAt = Date.now();
    try {
      services = (await api.services({})).services;
    } catch {
      servicesLoadedAt = 0;
    }
  }

  $effect(() => {
    if (ui.palette) {
      untrack(() => {
        q = "";
        sel = 0;
        stack = [];
        setTimeout(() => input?.focus(), 0);
        loadServices();
      });
    }
  });

  const nav = (href: string) => () => router.go(href);

  // ------------------------------------------------------------ host actions

  async function patchHost(h: HostSummary, patch: { muted_until?: number | "forever" | null; ignored?: boolean }, done: string) {
    try {
      await api.updateHost(h.id, patch);
      const live = fleet.hosts[h.id];
      if (live) Object.assign(live, patch);
      toast("ok", done, hostName(h));
    } catch (e) {
      toastError(e, `Could not update ${hostName(h)}`);
    }
  }

  async function setIgnored(h: HostSummary, ignored: boolean) {
    if (
      ignored &&
      !(await confirm({
        title: `Ignore ${hostName(h)}?`,
        body: "Monarch stops notifying about this host and leaves it out of fleet health. Its mute setting is kept.",
        confirm: "Ignore",
        danger: true,
      }))
    )
      return;
    await patchHost(h, { ignored }, ignored ? "Host ignored" : "Host no longer ignored");
  }

  function activeFailing(h: HostSummary): string[] {
    const muted = new Set(h.muted_services);
    return h.failing.filter((s) => !muted.has(s));
  }

  function hostActions(h: HostSummary): Item[] {
    const name = hostName(h);
    const g = name;
    const out: Item[] = [
      { id: `ha:${h.id}:open`, group: g, label: "Open host", icon: Server, run: nav(hostHref(h.id)) },
      { id: `ha:${h.id}:services`, group: g, label: "Services", icon: List, run: nav(hostHref(h.id, "services")) },
      { id: `ha:${h.id}:events`, group: g, label: "Events", icon: Activity, run: nav(hostHref(h.id, "events")) },
    ];
    if (can("admin")) out.push({ id: `ha:${h.id}:settings`, group: g, label: "Host settings", icon: Settings, run: nav(hostHref(h.id, "settings")) });

    const failing = activeFailing(h);
    if (can("operator") && h.can_act && h.online && failing.length) {
      const what = failing.length === 1 ? failing[0] : `${failing.length} failing services`;
      for (const action of ["restart", "remonitor"] as const) {
        out.push({
          id: `ha:${h.id}:${action}-failing`,
          group: "Failing services",
          label: `${ACTIONS.find((a) => a.id === action)!.label} ${what}`,
          hint: failing.length > 1 ? failing.join(", ") : undefined,
          icon: actionIcons[action],
          tone: "bad",
          run: () => runAction(h.id, name, failing, action),
        });
      }
    }

    if (can("admin")) {
      const m = "Notifications";
      const silenced = h.muted_until === "forever" || (typeof h.muted_until === "number" && h.muted_until > clock.now);
      if (silenced) {
        out.push({ id: `ha:${h.id}:unmute`, group: m, label: "Unmute notifications", icon: Bell, run: () => patchHost(h, { muted_until: null }, "Notifications unmuted") });
      }
      for (const [label, hours] of [["1 hour", 1], ["8 hours", 8], ["1 day", 24], ["1 week", 168]] as const) {
        out.push({
          id: `ha:${h.id}:mute:${hours}`,
          group: m,
          label: `Mute for ${label}`,
          icon: BellOff,
          run: () => patchHost(h, { muted_until: Math.floor(Date.now() / 1000) + hours * 3600 }, `Muted for ${label}`),
        });
      }
      if (h.muted_until !== "forever") out.push({ id: `ha:${h.id}:mute:forever`, group: m, label: "Mute forever", icon: BellOff, run: () => patchHost(h, { muted_until: "forever" }, "Muted forever") });
      out.push(
        h.ignored
          ? { id: `ha:${h.id}:unignore`, group: m, label: "Stop ignoring host", icon: Eye, run: () => setIgnored(h, false) }
          : { id: `ha:${h.id}:ignore`, group: m, label: "Ignore host", hint: "no notifications, left out of fleet health", icon: EyeOff, run: () => setIgnored(h, true) },
      );
    }
    return out;
  }

  // --------------------------------------------------------- service actions

  function canActOn(hostId: number): boolean {
    const h = fleet.hosts[hostId];
    return can("operator") && !!h?.can_act && h.online;
  }

  function serviceActionItem(s: ServiceListItem, action: ServiceAction, group: string, withName: boolean): Item {
    const a = ACTIONS.find((x) => x.id === action)!;
    return {
      id: `sa:${s.host_id}:${s.name}:${action}`,
      group,
      label: withName ? `${a.label} ${s.name}` : a.label,
      hint: withName ? s.host : undefined,
      icon: actionIcons[action],
      tone: a.danger ? "warn" : undefined,
      run: () => runAction(s.host_id, s.host, [s.name], action),
    };
  }

  function serviceActions(s: ServiceListItem): Item[] {
    const g = `${s.name} on ${s.host}`;
    const out: Item[] = [
      { id: `sa:${s.host_id}:${s.name}:open`, group: g, label: "Open service", icon: CornerDownLeft, run: nav(serviceHref(s.host_id, s.name)) },
      { id: `sa:${s.host_id}:${s.name}:host`, group: g, label: `Open host ${s.host}`, icon: Server, run: nav(hostHref(s.host_id)) },
    ];
    if (canActOn(s.host_id)) out.push(...ACTIONS.map((a) => serviceActionItem(s, a.id, "Monit actions", false)));
    return out;
  }

  // ------------------------------------------------------------------- items

  function hostItem(h: HostSummary, group: string): Item {
    const failing = activeFailing(h);
    return {
      id: `${group}:h:${h.id}`,
      group,
      label: hostName(h),
      hint: [
        !h.online || h.state === "offline" ? "offline" : failing.length ? `${failing.length} failing` : null,
        h.os.name,
        h.os.release,
        ...h.hostgroups,
      ]
        .filter(Boolean)
        .join(" · "),
      tone: hostTone(h.state),
      run: nav(hostHref(h.id)),
      sub: () => hostActions(h),
      subTitle: hostName(h),
    };
  }

  function serviceItem(s: ServiceListItem, group: string): Item {
    return {
      id: `${group}:s:${s.host_id}:${s.name}`,
      group,
      label: s.name,
      hint: [s.host, s.state !== "ok" ? s.status_text : null].filter(Boolean).join(" · "),
      serviceType: s.type,
      tone: s.stale ? "muted" : serviceTone(s.state),
      run: nav(serviceHref(s.host_id, s.name)),
      sub: () => serviceActions(s),
      subTitle: `${s.name} · ${s.host}`,
    };
  }

  const pages = $derived.by((): Item[] => {
    const out: Item[] = [
      { id: "p:overview", group: "Pages", label: "Overview", icon: LayoutDashboard, run: nav("/") },
      { id: "p:hosts", group: "Pages", label: "Hosts", icon: Server, run: nav("/hosts") },
      { id: "p:events", group: "Pages", label: "Events", icon: Activity, run: nav("/events") },
      { id: "p:settings", group: "Pages", label: "Settings", icon: Settings, run: nav("/settings") },
      { id: "p:profile", group: "Pages", label: "Settings › Profile", icon: Settings, run: nav("/settings/profile") },
      { id: "p:tokens", group: "Pages", label: "Settings › API tokens", icon: Settings, run: nav("/settings/tokens") },
      { id: "p:appearance", group: "Pages", label: "Settings › Appearance", hint: "theme, mode, favicon", icon: Settings, run: nav("/settings/appearance") },
    ];
    if (can("admin")) {
      out.push(
        { id: "p:users", group: "Pages", label: "Settings › Users", icon: Settings, run: nav("/settings/users") },
        { id: "p:notify", group: "Pages", label: "Settings › Notifications", icon: Settings, run: nav("/settings/notifications") },
        { id: "p:collector", group: "Pages", label: "Settings › Collector", icon: Settings, run: nav("/settings/collector") },
        { id: "p:hosts-settings", group: "Pages", label: "Settings › Hosts", icon: Settings, run: nav("/settings/hosts") },
      );
    }
    return out;
  });

  const actions = $derived.by((): Item[] => [
    { id: "a:addhost", group: "Actions", label: "Add host", hint: "push snippet or pull connection", icon: Plus, run: () => (ui.addHost = true) },
    { id: "a:theme", group: "Actions", label: theme.value === "dark" ? "Switch to light mode" : "Switch to dark mode", icon: theme.value === "dark" ? Sun : Moon, run: toggleTheme },
    ...themeStyles
      .filter((st) => st.id !== theme.style)
      .map((st): Item => ({ id: `a:style:${st.id}`, group: "Actions", label: `Use ${st.label} theme`, icon: Palette, run: () => setThemeStyle(st.id) })),
    { id: "a:sidebar", group: "Actions", label: "Toggle sidebar", icon: PanelLeft, run: toggleSidebar },
    { id: "a:logout", group: "Actions", label: "Sign out", icon: LogOut, run: onlogout },
  ]);

  const sortedHosts = $derived(hostList());
  const hosts = $derived(sortedHosts.map((h) => hostItem(h, "Hosts")));
  const svcs = $derived(services.map((s) => serviceItem(s, "Services")));

  /** Offline hosts and failed services that still notify, worst first. */
  const failing = $derived.by((): Item[] => {
    const now = clock.now;
    const offline = sortedHosts.filter((h) => !isHostSilenced(h, now) && (!h.online || h.state === "offline")).map((h) => hostItem(h, "Failing"));
    const failed = services
      .filter((s) => {
        const h = fleet.hosts[s.host_id];
        return s.state === "failed" && !s.stale && !!h && !isHostSilenced(h, now) && !h.muted_services.includes(s.name);
      })
      .map((s) => serviceItem(s, "Failing"));
    return [...offline, ...failed];
  });

  function rank(list: Item[], term: string, limit = 40): Item[] {
    return list
      .map((it) => ({ it, s: Math.max(fuzzy(term, it.label), fuzzy(term, `${it.hint ?? ""} ${it.label}`) - 50) }))
      .filter((x) => x.s >= 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, limit)
      .map((x) => x.it);
  }

  const scope = $derived(stack.length ? undefined : SCOPES.find((s) => q.startsWith(s.prefix)));
  const term = $derived((scope ? q.slice(scope.prefix.length) : q).trim());

  /** "restart nginx" lists the matching services' restart actions directly. */
  const verbItems = $derived.by((): Item[] => {
    const m = term.match(/^(\S+)\s+(.+)$/);
    if (!m || !can("operator")) return [];
    const action = ACTIONS.find((a) => a.id === m[1].toLowerCase());
    if (!action) return [];
    const matches = services.filter((s) => canActOn(s.host_id)).map((s) => serviceActionItem(s, action.id, "Monit actions", true));
    return rank(matches, m[2], 12);
  });

  const items = $derived.by((): Item[] => {
    const frame = stack.at(-1);
    if (frame) return term ? rank(frame.items(), term) : frame.items();
    if (scope) {
      const pool = scope.prefix === ">" ? [...pages, ...actions] : scope.prefix === "@" ? hosts : scope.prefix === "#" ? svcs : failing;
      return term ? rank(pool, term) : pool.slice(0, 40);
    }
    if (!term) return [...failing.slice(0, 6), ...pages, ...hosts.slice(0, 8), ...actions];
    return [...verbItems, ...rank([...pages, ...hosts, ...svcs, ...actions], term)];
  });

  const groups = $derived.by(() => {
    const g = new Map<string, { item: Item; index: number }[]>();
    items.forEach((item, index) => {
      if (!g.has(item.group)) g.set(item.group, []);
      g.get(item.group)!.push({ item, index });
    });
    return [...g.entries()];
  });

  function open(i: Item) {
    if (!i.sub) return;
    stack = [...stack, { title: i.subTitle ?? i.label, items: i.sub }];
    q = "";
    sel = 0;
    input?.focus();
  }

  function back() {
    stack = stack.slice(0, -1);
    q = "";
    sel = 0;
    input?.focus();
  }

  function run(i: Item) {
    if (!i.run) return open(i);
    ui.palette = false;
    i.run();
  }

  function onkey(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      ui.palette = !ui.palette;
      return;
    }
    if (!ui.palette) {
      if (e.key === "/" && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || (e.target as HTMLElement | null)?.isContentEditable)) {
        e.preventDefault();
        ui.palette = true;
      }
      return;
    }
    const caretAtEnd = !input || (input.selectionStart === q.length && input.selectionEnd === q.length);
    if (e.key === "Escape") {
      e.preventDefault();
      if (stack.length) back();
      else ui.palette = false;
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      sel = Math.min(items.length - 1, sel + 1);
      scrollSel();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      sel = Math.max(0, sel - 1);
      scrollSel();
    } else if ((e.key === "Tab" && !e.shiftKey) || (e.key === "ArrowRight" && caretAtEnd)) {
      if (items[sel]?.sub) {
        e.preventDefault();
        open(items[sel]);
      } else if (e.key === "Tab") e.preventDefault();
    } else if ((e.key === "Backspace" && q === "" && stack.length) || (e.key === "Tab" && e.shiftKey) || (e.key === "ArrowLeft" && q === "" && stack.length)) {
      e.preventDefault();
      if (stack.length) back();
    } else if (e.key === "Enter" && items[sel]) {
      e.preventDefault();
      run(items[sel]);
    }
  }
  function scrollSel() {
    queueMicrotask(() => document.querySelector(`[data-pal="${sel}"]`)?.scrollIntoView({ block: "nearest" }));
  }
  $effect(() => {
    void q;
    sel = 0;
  });

  const placeholder = $derived(
    stack.length
      ? `Actions for ${stack.at(-1)!.title}…`
      : scope
        ? `Search ${scope.label}…`
        : "Search hosts, services, actions… (try “restart nginx”)",
  );
</script>

<svelte:window onkeydown={onkey} />

{#if ui.palette}
  <div class="fixed inset-0 z-[65] flex items-start justify-center px-4 pt-[12vh]">
    <button class="absolute inset-0 cursor-default bg-[var(--mn-scrim)] backdrop-blur-sm" aria-label="Close" onclick={() => (ui.palette = false)}></button>
    <div class="relative w-full max-w-xl overflow-hidden rounded-2xl border border-line-strong bg-surface-solid shadow-2xl animate-in" role="dialog" aria-label="Command palette">
      <div class="flex items-center gap-3 border-b border-line px-4">
        <Search size={17} class="shrink-0 text-fg-3" />
        {#each stack as frame, i (i)}
          <button
            class="flex max-w-[45%] shrink-0 items-center gap-1 rounded-lg border border-line bg-surface-2 py-1 pr-1.5 pl-2 text-[12px] font-medium text-fg-2 hover:text-fg"
            title="Back (Backspace)"
            onclick={() => {
              stack = stack.slice(0, i);
              q = "";
              input?.focus();
            }}
          >
            <span class="truncate">{frame.title}</span>
            <X size={12} class="shrink-0" />
          </button>
        {/each}
        {#if scope}<span class="kbd shrink-0" title="Scope: {scope.label}">{scope.prefix} {scope.label}</span>{/if}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          autofocus
          bind:this={input}
          bind:value={q}
          class="h-14 min-w-0 flex-1 bg-transparent text-[15px] text-fg outline-none placeholder:text-fg-3"
          {placeholder}
          aria-label="Search"
        />
        <span class="kbd shrink-0">esc</span>
      </div>
      <div class="max-h-[52vh] overflow-y-auto p-2">
        {#if items.length === 0}
          <div class="px-3 py-10 text-center text-sm text-fg-3">
            {#if scope?.prefix === "!" && !term}Nothing is failing{:else}No results for “{term || q}”{/if}
          </div>
        {/if}
        {#each groups as [group, entries] (group)}
          <div class="px-2.5 pt-2.5 pb-1 eyebrow">{group}</div>
          {#each entries as { item, index } (item.id)}
            <button
              data-pal={index}
              class="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-left transition-colors {sel === index ? 'bg-hover ring-1 ring-[var(--mn-line-strong)]' : ''}"
              onmouseenter={() => (sel = index)}
              onclick={() => run(item)}
            >
              <span class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-line bg-surface-2 text-fg-2 {item.icon && item.tone ? `tone-${item.tone} text-tone` : ''}">
                {#if item.serviceType}
                  <ServiceIcon type={item.serviceType} size={14} />
                {:else if item.icon}
                  <item.icon size={14} />
                {:else}
                  <StatusDot tone={item.tone ?? "muted"} />
                {/if}
              </span>
              <span class="min-w-0 flex-1">
                <span class="block truncate text-[13px] font-medium text-fg">{item.label}</span>
                {#if item.hint}<span class="block truncate text-[11px] text-fg-3">{item.hint}</span>{/if}
              </span>
              {#if item.tone && !item.icon}<StatusDot tone={item.tone} size={7} />{/if}
              {#if sel === index}
                {#if item.sub}
                  <span class="flex items-center gap-1 text-[11px] text-fg-3"><span class="kbd">⇥</span> actions</span>
                {/if}
                {#if item.run}<CornerDownLeft size={14} class="text-fg-3" />{:else}<ChevronRight size={14} class="text-fg-3" />{/if}
              {/if}
            </button>
          {/each}
        {/each}
      </div>
      <div class="flex flex-wrap items-center gap-x-4 gap-y-1.5 border-t border-line px-4 py-2.5 text-[11px] text-fg-3">
        <span class="flex items-center gap-1"><span class="kbd">↑</span><span class="kbd">↓</span> navigate</span>
        <span class="flex items-center gap-1"><span class="kbd">↵</span> open</span>
        <span class="flex items-center gap-1"><span class="kbd">⇥</span> actions</span>
        {#if stack.length}
          <span class="flex items-center gap-1"><span class="kbd">⌫</span> back</span>
        {:else}
          <span class="ml-auto flex items-center gap-2">
            {#each SCOPES as s (s.prefix)}
              <button class="flex items-center gap-1 hover:text-fg" onclick={() => { q = s.prefix; input?.focus(); }}><span class="kbd">{s.prefix}</span>{s.label}</button>
            {/each}
          </span>
        {/if}
      </div>
    </div>
  </div>
{/if}
