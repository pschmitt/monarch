<script lang="ts">
  import { ArrowDown, ArrowDownToLine, ArrowUp, LayoutGrid, List, Plus, Search, ServerOff } from "@lucide/svelte";
  import type { HostSummary } from "../lib/types";
  import { ago, duration, hostName, hostStateLabel, hostTone, osLabel, pct } from "../lib/format";
  import { fuzzy } from "../lib/fuzzy";
  import { clock, fleet, ui } from "../lib/state.svelte";
  import { router, hostHref } from "../lib/router.svelte";
  import Badge from "../lib/components/Badge.svelte";
  import Empty from "../lib/components/Empty.svelte";
  import HostCard from "../lib/components/HostCard.svelte";
  import Sparkline from "../lib/components/Sparkline.svelte";
  import StatusDot from "../lib/components/StatusDot.svelte";
  import UsageBar from "../lib/components/UsageBar.svelte";

  type SortKey = "name" | "state" | "cpu" | "mem" | "load" | "uptime" | "seen" | "services";

  const q0 = router.route.query;
  let q = $state(q0.get("q") ?? "");
  let stateFilter = $state(q0.get("state") ?? "");
  let group = $state(q0.get("group") ?? "");
  let view = $state<"table" | "grid">((localStorage.getItem("monarch.hostsView") as "table" | "grid") ?? "table");
  let sort = $state<SortKey>((q0.get("sort") as SortKey) ?? "state");
  let dir = $state<1 | -1>(q0.get("dir") === "desc" ? -1 : 1);

  $effect(() => {
    try {
      localStorage.setItem("monarch.hostsView", view);
    } catch {
      /* ignore */
    }
  });
  $effect(() => {
    router.setQuery({ q, state: stateFilter, group, sort: sort === "state" ? null : sort, dir: dir === -1 ? "desc" : null });
  });

  const groups = $derived([...new Set(Object.values(fleet.hosts).flatMap((h) => h.hostgroups))].sort());
  const stateRank = { offline: 0, degraded: 1, ok: 2 } as const;

  const val = (h: HostSummary, k: SortKey): number | string => {
    switch (k) {
      case "name":
        return hostName(h).toLowerCase();
      case "state":
        return stateRank[h.state];
      case "cpu":
        return h.system.cpu ?? -1;
      case "mem":
        return h.system.mem_percent ?? -1;
      case "load":
        return h.system.load?.[0] ?? -1;
      case "uptime":
        return h.system.uptime ?? -1;
      case "seen":
        return h.last_seen;
      case "services":
        return h.services.failed * 1000 + h.services.total;
    }
  };

  const rows = $derived.by(() => {
    let list = Object.values(fleet.hosts);
    if (stateFilter) list = list.filter((h) => h.state === stateFilter);
    if (group) list = list.filter((h) => h.hostgroups.includes(group));
    if (q.trim()) {
      list = list
        .map((h) => ({ h, s: fuzzy(q, `${hostName(h)} ${h.hostname} ${osLabel(h.os)} ${h.hostgroups.join(" ")} ${h.description ?? ""}`) }))
        .filter((x) => x.s >= 0)
        .map((x) => x.h);
    }
    return list.sort((a, b) => {
      const va = val(a, sort);
      const vb = val(b, sort);
      const c = va < vb ? -1 : va > vb ? 1 : 0;
      return c * dir || hostName(a).localeCompare(hostName(b), undefined, { numeric: true });
    });
  });

  const counts = $derived.by(() => {
    const list = Object.values(fleet.hosts);
    return {
      all: list.length,
      ok: list.filter((h) => h.state === "ok").length,
      degraded: list.filter((h) => h.state === "degraded").length,
      offline: list.filter((h) => h.state === "offline").length,
    };
  });

  function setSort(k: SortKey) {
    if (sort === k) dir = dir === 1 ? -1 : 1;
    else {
      sort = k;
      dir = k === "name" || k === "state" ? 1 : -1;
    }
  }
</script>

{#snippet th(k: SortKey, label: string, cls = "")}
  <th class={cls}>
    <button class="inline-flex items-center gap-1 uppercase hover:text-fg-2 {sort === k ? 'text-fg-2' : ''}" onclick={() => setSort(k)}>
      {label}
      {#if sort === k}{#if dir === 1}<ArrowUp size={11} />{:else}<ArrowDown size={11} />{/if}{/if}
    </button>
  </th>
{/snippet}

<div class="space-y-5">
  <div class="flex flex-wrap items-end justify-between gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight text-fg">Hosts</h1>
      <p class="mt-1 text-[13px] text-fg-3">Every Monit agent reporting to — or polled by — this Monarch instance.</p>
    </div>
    <button class="btn btn-primary" onclick={() => (ui.addHost = true)}><Plus size={15} /> Add host</button>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <div class="relative min-w-56 flex-1 sm:max-w-xs">
      <Search size={15} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-fg-3" />
      <input class="input pl-9" placeholder="Filter hosts…" bind:value={q} aria-label="Filter hosts" />
    </div>
    <div class="flex rounded-xl border border-line bg-surface-2 p-0.5 text-xs">
      {#each [["", "All", counts.all], ["ok", "Healthy", counts.ok], ["degraded", "Degraded", counts.degraded], ["offline", "Offline", counts.offline]] as [id, label, n] (id)}
        <button
          class="flex h-8 items-center gap-1.5 rounded-[9px] px-2.5 font-medium transition-colors {stateFilter === id ? 'bg-surface-solid text-fg shadow-sm ring-1 ring-[var(--line-strong)]' : 'text-fg-3 hover:text-fg-2'}"
          onclick={() => (stateFilter = id as string)}
        >
          {#if id}<StatusDot tone={hostTone(id as "ok")} size={6} />{/if}{label}<span class="num text-[10px] text-fg-3">{n}</span>
        </button>
      {/each}
    </div>
    {#if groups.length}
      <select class="input h-9 w-auto min-w-36 text-xs" bind:value={group} aria-label="Host group">
        <option value="">All groups</option>
        {#each groups as g (g)}<option value={g}>{g}</option>{/each}
      </select>
    {/if}
    <div class="ml-auto flex rounded-xl border border-line bg-surface-2 p-0.5">
      <button class="flex h-8 w-8 items-center justify-center rounded-[9px] {view === 'table' ? 'bg-surface-solid text-fg ring-1 ring-[var(--line-strong)]' : 'text-fg-3'}" onclick={() => (view = "table")} aria-label="Table view"><List size={15} /></button>
      <button class="flex h-8 w-8 items-center justify-center rounded-[9px] {view === 'grid' ? 'bg-surface-solid text-fg ring-1 ring-[var(--line-strong)]' : 'text-fg-3'}" onclick={() => (view = "grid")} aria-label="Grid view"><LayoutGrid size={15} /></button>
    </div>
  </div>

  {#if !fleet.loaded}
    <div class="card space-y-3 p-4">{#each Array(6) as _, i (i)}<div class="skeleton h-11 w-full"></div>{/each}</div>
  {:else if rows.length === 0}
    <div class="card">
      <Empty icon={ServerOff} title={counts.all ? "No hosts match" : "No hosts yet"} body={counts.all ? "Try a different filter." : "Point a Monit agent at the collector to get started."}>
        {#if !counts.all}<button class="btn btn-primary" onclick={() => (ui.addHost = true)}><Plus size={15} /> Add your first host</button>{/if}
      </Empty>
    </div>
  {:else if view === "grid"}
    <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
      {#each rows as h (h.id)}<HostCard host={h} />{/each}
    </div>
  {:else}
    <div class="card overflow-x-auto">
      <table class="table min-w-[900px]">
        <thead>
          <tr>
            {@render th("name", "Host", "pl-5")}
            {@render th("state", "Status")}
            {@render th("services", "Services")}
            {@render th("cpu", "CPU", "w-40")}
            {@render th("mem", "Memory", "w-40")}
            {@render th("load", "Load")}
            {@render th("uptime", "Uptime")}
            {@render th("seen", "Last report", "pr-5 text-right")}
          </tr>
        </thead>
        <tbody>
          {#each rows as h (h.id)}
            <tr class="cursor-pointer" onclick={() => router.go(hostHref(h.id))}>
              <td class="pl-5">
                <a href={hostHref(h.id)} class="flex items-center gap-3" onclick={(e) => e.stopPropagation()}>
                  <StatusDot tone={hostTone(h.state)} pulse={h.state !== "ok"} />
                  <div class="min-w-0">
                    <div class="flex items-center gap-1.5 truncate font-semibold text-fg">{hostName(h)}{#if h.source === "pull"}<span title="Polled by Monarch" class="text-fg-3"><ArrowDownToLine size={12} /></span>{/if}</div>
                    <div class="truncate text-[11px] text-fg-3">{osLabel(h.os)}{h.hostgroups.length ? ` · ${h.hostgroups.join(", ")}` : ""}</div>
                  </div>
                </a>
              </td>
              <td><Badge tone={hostTone(h.state)} size="sm">{hostStateLabel[h.state]}</Badge></td>
              <td>
                <div class="num text-xs">
                  {#if h.services.failed}<span class="font-semibold text-bad">{h.services.failed} failed</span><span class="text-fg-3"> · </span>{/if}
                  <span class="text-fg-2">{h.services.total}</span>
                </div>
                {#if h.failing.length}<div class="max-w-48 truncate text-[11px] text-bad/80">{h.failing.join(", ")}</div>{/if}
              </td>
              <td>
                <div class="flex items-center gap-2.5">
                  <div class="w-16"><Sparkline values={h.sparkline.cpu} max={100} height={22} /></div>
                  <span class="num w-12 text-right text-xs text-fg">{pct(h.system.cpu)}</span>
                </div>
              </td>
              <td>
                <div class="flex items-center gap-2.5">
                  <div class="w-16"><UsageBar value={h.system.mem_percent} height={5} /></div>
                  <span class="num w-12 text-right text-xs text-fg">{pct(h.system.mem_percent)}</span>
                </div>
              </td>
              <td class="num text-xs text-fg-2">{h.system.load ? h.system.load.map((l) => l.toFixed(2)).join("  ") : "—"}</td>
              <td class="num text-xs text-fg-2">{duration(h.system.uptime)}</td>
              <td class="num pr-5 text-right text-xs {h.online ? 'text-fg-3' : 'text-bad'}">{ago(h.last_seen, clock.now)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
