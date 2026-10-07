<script lang="ts">
  import { untrack, type Component } from "svelte";
  import {
    Activity,
    CornerDownLeft,
    LayoutDashboard,
    LogOut,
    Moon,
    PanelLeft,
    Search,
    Server,
    Settings,
    Sun,
  } from "@lucide/svelte";
  import { api } from "../api";
  import { fuzzy } from "../fuzzy";
  import { hostName, hostTone, serviceTone } from "../format";
  import { router, hostHref, serviceHref } from "../router.svelte";
  import { can, fleet, hostList, theme, toggleSidebar, toggleTheme, ui } from "../state.svelte";
  import type { Service } from "../types";
  import ServiceIcon from "./ServiceIcon.svelte";
  import StatusDot from "./StatusDot.svelte";

  let { onlogout }: { onlogout: () => void } = $props();

  interface Item {
    id: string;
    group: string;
    label: string;
    hint?: string;
    icon?: Component<any>;
    service?: Service;
    tone?: "ok" | "warn" | "bad" | "muted" | "info";
    run: () => void;
  }

  let q = $state("");
  let sel = $state(0);
  let input = $state<HTMLInputElement>();
  let services = $state<{ hostId: number; host: string; svc: Service }[]>([]);
  let servicesLoadedAt = 0;

  async function loadServices() {
    if (Date.now() - servicesLoadedAt < 60_000) return;
    servicesLoadedAt = Date.now();
    const hosts = hostList();
    const out: typeof services = [];
    const queue = [...hosts];
    await Promise.all(
      Array.from({ length: 4 }, async () => {
        while (queue.length) {
          const h = queue.shift()!;
          try {
            const d = await api.host(h.id);
            for (const s of d.services) out.push({ hostId: h.id, host: hostName(h), svc: s });
          } catch {
            /* ignore */
          }
        }
      }),
    );
    services = out;
  }

  $effect(() => {
    if (ui.palette) {
      untrack(() => {
        q = "";
        sel = 0;
        setTimeout(() => input?.focus(), 0);
        loadServices();
      });
    }
  });

  const nav = (href: string) => () => router.go(href);

  const items = $derived.by((): Item[] => {
    const base: Item[] = [
      { id: "p:overview", group: "Pages", label: "Overview", icon: LayoutDashboard, run: nav("/") },
      { id: "p:hosts", group: "Pages", label: "Hosts", icon: Server, run: nav("/hosts") },
      { id: "p:events", group: "Pages", label: "Events", icon: Activity, run: nav("/events") },
    ];
    if (can("admin")) {
      base.push(
        { id: "p:settings", group: "Pages", label: "Settings", icon: Settings, run: nav("/settings") },
        { id: "p:users", group: "Pages", label: "Settings › Users", icon: Settings, run: nav("/settings/users") },
        { id: "p:notify", group: "Pages", label: "Settings › Notifications", icon: Settings, run: nav("/settings/notifications") },
        { id: "p:collector", group: "Pages", label: "Settings › Collector", icon: Settings, run: nav("/settings/collector") },
      );
    }
    const actions: Item[] = [
      { id: "a:theme", group: "Actions", label: theme.value === "dark" ? "Switch to light theme" : "Switch to dark theme", icon: theme.value === "dark" ? Sun : Moon, run: toggleTheme },
      { id: "a:sidebar", group: "Actions", label: "Toggle sidebar", icon: PanelLeft, run: toggleSidebar },
      { id: "a:logout", group: "Actions", label: "Sign out", icon: LogOut, run: onlogout },
    ];
    const hosts: Item[] = Object.values(fleet.hosts).map((h) => ({
      id: `h:${h.id}`,
      group: "Hosts",
      label: hostName(h),
      hint: [h.os.name, h.os.release, ...h.hostgroups].filter(Boolean).join(" · "),
      tone: hostTone(h.state),
      run: nav(hostHref(h.id)),
    }));
    const svcs: Item[] = services.map(({ hostId, host, svc }) => ({
      id: `s:${hostId}:${svc.name}`,
      group: "Services",
      label: svc.name,
      hint: host,
      service: svc,
      tone: serviceTone(svc.state),
      run: nav(serviceHref(hostId, svc.name)),
    }));
    const all = [...base, ...hosts, ...svcs, ...actions];
    if (!q.trim()) return [...base, ...hosts.slice(0, 8), ...actions];
    return all
      .map((it) => ({ it, s: Math.max(fuzzy(q, it.label), fuzzy(q, `${it.hint ?? ""} ${it.label}`) - 50) }))
      .filter((x) => x.s >= 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 40)
      .map((x) => x.it);
  });

  const groups = $derived.by(() => {
    const g = new Map<string, { item: Item; index: number }[]>();
    items.forEach((item, index) => {
      if (!g.has(item.group)) g.set(item.group, []);
      g.get(item.group)!.push({ item, index });
    });
    return [...g.entries()];
  });

  function run(i: Item) {
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
      if (e.key === "/" && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) {
        e.preventDefault();
        ui.palette = true;
      }
      return;
    }
    if (e.key === "Escape") ui.palette = false;
    else if (e.key === "ArrowDown") {
      e.preventDefault();
      sel = Math.min(items.length - 1, sel + 1);
      scrollSel();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      sel = Math.max(0, sel - 1);
      scrollSel();
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
</script>

<svelte:window onkeydown={onkey} />

{#if ui.palette}
  <div class="fixed inset-0 z-[65] flex items-start justify-center px-4 pt-[12vh]">
    <button class="absolute inset-0 cursor-default bg-[var(--scrim)] backdrop-blur-sm" aria-label="Close" onclick={() => (ui.palette = false)}></button>
    <div class="relative w-full max-w-xl overflow-hidden rounded-2xl border border-line-strong bg-surface-solid shadow-2xl animate-in" role="dialog" aria-label="Command palette">
      <div class="flex items-center gap-3 border-b border-line px-4">
        <Search size={17} class="text-fg-3" />
        <!-- svelte-ignore a11y_autofocus -->
        <input
          autofocus
          bind:this={input}
          bind:value={q}
          class="h-14 flex-1 bg-transparent text-[15px] text-fg outline-none placeholder:text-fg-3"
          placeholder="Search hosts, services, pages…"
          aria-label="Search"
        />
        <span class="kbd">esc</span>
      </div>
      <div class="max-h-[52vh] overflow-y-auto p-2">
        {#if items.length === 0}
          <div class="px-3 py-10 text-center text-sm text-fg-3">No results for “{q}”</div>
        {/if}
        {#each groups as [group, entries] (group)}
          <div class="px-2.5 pt-2.5 pb-1 eyebrow">{group}</div>
          {#each entries as { item, index } (item.id)}
            <button
              data-pal={index}
              class="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-left transition-colors {sel === index ? 'bg-hover ring-1 ring-[var(--line-strong)]' : ''}"
              onmouseenter={() => (sel = index)}
              onclick={() => run(item)}
            >
              <span class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-line bg-surface-2 text-fg-2">
                {#if item.service}
                  <ServiceIcon type={item.service.type} size={14} />
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
              {#if item.tone && (item.service || item.group === "Hosts")}<StatusDot tone={item.tone} size={7} />{/if}
              {#if sel === index}<CornerDownLeft size={14} class="text-fg-3" />{/if}
            </button>
          {/each}
        {/each}
      </div>
      <div class="flex items-center gap-4 border-t border-line px-4 py-2.5 text-[11px] text-fg-3">
        <span class="flex items-center gap-1"><span class="kbd">↑</span><span class="kbd">↓</span> navigate</span>
        <span class="flex items-center gap-1"><span class="kbd">↵</span> open</span>
        <span class="ml-auto flex items-center gap-1"><span class="kbd">/</span> or <span class="kbd">⌘K</span></span>
      </div>
    </div>
  </div>
{/if}
