<script lang="ts">
  import { Activity, ArrowRight, CircleCheckBig, Cpu, HardDrive, MemoryStick, Server, Siren, Sparkles } from "@lucide/svelte";
  import { api } from "../lib/api";
  import type { MonarchEvent, Overview } from "../lib/types";
  import { ago, compact, kb, mb, pct, serviceTypeLabel, timeShort, usageTone } from "../lib/format";
  import { clock, fleet, hostList, toastError } from "../lib/state.svelte";
  import { serviceHref } from "../lib/router.svelte";
  import Empty from "../lib/components/Empty.svelte";
  import EventRow from "../lib/components/EventRow.svelte";
  import HostCard from "../lib/components/HostCard.svelte";
  import ServiceIcon from "../lib/components/ServiceIcon.svelte";
  import Skeleton from "../lib/components/Skeleton.svelte";
  import StatusDot from "../lib/components/StatusDot.svelte";
  import UsageBar from "../lib/components/UsageBar.svelte";

  let data = $state<Overview | null>(null);
  let loadedAt = $state(0);

  async function load() {
    try {
      data = await api.overview();
      loadedAt = Date.now() / 1000;
    } catch (e) {
      toastError(e, "Failed to load overview");
    }
  }

  $effect(() => {
    load();
    const t = setInterval(load, 30_000);
    return () => clearInterval(t);
  });

  // Debounced refresh when live updates arrive.
  let pending: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void fleet.tick;
    if (!data) return;
    if (pending) clearTimeout(pending);
    pending = setTimeout(load, 4000);
    return () => pending && clearTimeout(pending);
  });

  const hosts = $derived(
    hostList().sort((a, b) => {
      const rank = { offline: 0, degraded: 1, ok: 2 } as const;
      return rank[a.state] - rank[b.state] || a.hostname.localeCompare(b.hostname, undefined, { numeric: true });
    }),
  );

  // Live hosts take precedence over the (possibly stale) overview snapshot.
  const hostCounts = $derived.by(() => {
    const list = Object.values(fleet.hosts);
    if (!list.length && data) return data.hosts;
    return {
      total: list.length,
      online: list.filter((h) => h.online).length,
      offline: list.filter((h) => !h.online).length,
      degraded: list.filter((h) => h.state === "degraded").length,
    };
  });

  const recent = $derived.by((): MonarchEvent[] => {
    const seen = new Set<number>();
    const out: MonarchEvent[] = [];
    for (const e of [...fleet.live, ...(data?.recent_events ?? [])]) {
      if (seen.has(e.id)) continue;
      seen.add(e.id);
      out.push(e);
    }
    return out.sort((a, b) => b.created_at - a.created_at).slice(0, 12);
  });

  const activityMax = $derived(Math.max(1, ...(data?.activity ?? []).map((a) => a.ok + a.failed)));
  const healthPct = $derived(data && data.services.total ? (data.services.ok / data.services.total) * 100 : null);
  const greeting = $derived.by(() => {
    const h = new Date(clock.now * 1000).getHours();
    return h < 5 ? "Burning the midnight oil" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
  });
</script>

<div class="space-y-6">
  <!-- header -->
  <div class="flex flex-wrap items-end justify-between gap-4">
    <div>
      <div class="eyebrow">{greeting}</div>
      <h1 class="mt-1 text-2xl font-semibold tracking-tight text-fg sm:text-[28px]">
        {#if !data}
          Fleet overview
        {:else if data.services.failed === 0 && hostCounts.offline === 0}
          Everything is <span class="text-gradient">running smoothly</span>
        {:else}
          <span class="text-bad">{data.services.failed + hostCounts.offline}</span> thing{data.services.failed + hostCounts.offline === 1 ? "" : "s"} need{data.services.failed + hostCounts.offline === 1 ? "s" : ""} your attention
        {/if}
      </h1>
    </div>
    <div class="num text-[11px] text-fg-3">{loadedAt ? `updated ${ago(loadedAt, clock.now)}` : ""}</div>
  </div>

  <!-- KPI tiles -->
  <div class="grid grid-cols-2 gap-3 lg:grid-cols-4 lg:gap-4">
    <a href="/hosts" class="card card-hover kpi relative overflow-hidden p-4 sm:p-5" style="--kpi: var(--accent)">
      <div class="flex items-center justify-between">
        <span class="eyebrow">Hosts</span>
        <span class="kpi-icon"><Server size={16} /></span>
      </div>
      <div class="mt-3 flex items-baseline gap-1.5">
        <span class="num text-2xl font-semibold tracking-tight sm:text-3xl text-fg">{hostCounts.online}</span>
        <span class="num text-sm text-fg-3">/ {hostCounts.total} online</span>
      </div>
      <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px]">
        <span class="flex items-center gap-1.5 {hostCounts.degraded ? 'text-warn' : 'text-fg-3'}"><StatusDot tone={hostCounts.degraded ? "warn" : "muted"} size={6} />{hostCounts.degraded} degraded</span>
        <span class="flex items-center gap-1.5 {hostCounts.offline ? 'text-bad' : 'text-fg-3'}"><StatusDot tone={hostCounts.offline ? "bad" : "muted"} size={6} pulse={hostCounts.offline > 0} />{hostCounts.offline} offline</span>
      </div>
    </a>

    <div class="card kpi relative overflow-hidden p-4 sm:p-5" style="--kpi: var(--ok)">
      <div class="flex items-center justify-between">
        <span class="eyebrow">Services</span>
        <span class="kpi-icon"><CircleCheckBig size={16} /></span>
      </div>
      {#if data}
        <div class="mt-3 flex items-baseline gap-1.5">
          <span class="num text-2xl font-semibold tracking-tight sm:text-3xl text-fg">{data.services.ok}</span>
          <span class="num text-sm text-fg-3">/ {data.services.total} healthy</span>
        </div>
        <div class="mt-2.5"><UsageBar value={healthPct} tone={data.services.failed ? "warn" : "ok"} height={4} /></div>
        <div class="mt-2 flex flex-wrap gap-x-3 text-[11px] text-fg-3">
          <span>{data.services.unmonitored} unmonitored</span><span>{data.services.pending} pending</span>
        </div>
      {:else}<Skeleton class="mt-3 h-9 w-24" /><Skeleton class="mt-3 h-3 w-full" />{/if}
    </div>

    <div class="card kpi relative overflow-hidden p-4 sm:p-5" style="--kpi: {data?.services.failed ? 'var(--bad)' : 'var(--fg-3)'}">
      <div class="flex items-center justify-between">
        <span class="eyebrow">Failing</span>
        <span class="kpi-icon"><Siren size={16} /></span>
      </div>
      {#if data}
        <div class="mt-3 flex items-baseline gap-1.5">
          <span class="num text-2xl font-semibold tracking-tight sm:text-3xl {data.services.failed ? 'text-bad' : 'text-fg'}">{data.services.failed}</span>
          <span class="text-sm text-fg-3">service{data.services.failed === 1 ? "" : "s"}</span>
        </div>
        <div class="mt-2 truncate text-[11px] text-fg-3">
          {#if data.failing[0]}longest: {data.failing[0].host} / {data.failing[0].service}{:else}nothing is on fire 🎉{/if}
        </div>
      {:else}<Skeleton class="mt-3 h-9 w-16" />{/if}
    </div>

    <a href="/events" class="card card-hover kpi relative overflow-hidden p-4 sm:p-5" style="--kpi: var(--accent-2)">
      <div class="flex items-center justify-between">
        <span class="eyebrow">Events · 24h</span>
        <span class="kpi-icon"><Activity size={16} /></span>
      </div>
      {#if data}
        <div class="mt-3 flex items-end justify-between gap-3">
          <span class="num text-2xl font-semibold tracking-tight sm:text-3xl text-fg">{compact(data.events_24h)}</span>
          <svg viewBox="0 0 96 28" class="h-7 w-24" aria-hidden="true">
            {#each data.activity as a, i (a.ts)}
              {@const h = ((a.ok + a.failed) / activityMax) * 26}
              {@const hf = (a.failed / activityMax) * 26}
              <rect x={i * 4} y={28 - h} width="3" height={Math.max(h - hf, 0)} rx="1" fill="var(--accent-2)" opacity="0.55" />
              <rect x={i * 4} y={28 - hf} width="3" height={hf} rx="1" fill="var(--bad)" />
            {/each}
          </svg>
        </div>
        <div class="mt-2 text-[11px] text-fg-3">
          <span class="text-bad">{data.activity.reduce((s, a) => s + a.failed, 0)}</span> failures ·
          <span>{data.activity.reduce((s, a) => s + a.ok, 0)}</span> recoveries & changes
        </div>
      {:else}<Skeleton class="mt-3 h-9 w-20" />{/if}
    </a>
  </div>

  <div class="grid gap-6 xl:grid-cols-[minmax(0,1fr)_380px]">
    <div class="min-w-0 space-y-6">
      <!-- needs attention -->
      {#if data && data.failing.length}
        <section class="card tone-bad overflow-hidden">
          <div class="flex items-center justify-between border-b border-line px-5 py-3.5">
            <div class="flex items-center gap-2.5">
              <StatusDot tone="bad" pulse />
              <h2 class="card-title">Needs attention</h2>
              <span class="num rounded-md bg-tone-soft px-1.5 text-[11px] font-semibold text-tone">{data.failing.length}</span>
            </div>
          </div>
          <ul class="divide-y divide-[var(--line)]">
            {#each data.failing as f (f.host_id + f.service)}
              <li>
                <a href={serviceHref(f.host_id, f.service)} class="group flex items-center gap-3 px-5 py-3 transition-colors hover:bg-hover">
                  <span class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-tone-soft text-tone"><ServiceIcon type={f.type} size={15} /></span>
                  <div class="min-w-0 flex-1">
                    <div class="truncate text-[13px]"><span class="font-semibold text-fg">{f.service}</span> <span class="text-fg-3">on</span> <span class="text-fg-2">{f.host}</span></div>
                    <div class="truncate text-xs text-tone">{f.status_text} <span class="text-fg-3">· {serviceTypeLabel[f.type]}</span></div>
                  </div>
                  <div class="num text-right text-[11px] text-fg-3">
                    {#if f.since}failing for<br /><span class="text-fg-2">{ago(f.since, clock.now).replace(" ago", "")}</span>{/if}
                  </div>
                  <ArrowRight size={15} class="text-fg-3 transition-transform group-hover:translate-x-0.5" />
                </a>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <!-- fleet -->
      <section>
        <div class="mb-3 flex items-center justify-between">
          <h2 class="card-title">Fleet</h2>
          <a href="/hosts" class="flex items-center gap-1 text-xs text-fg-3 hover:text-fg">All hosts <ArrowRight size={13} /></a>
        </div>
        {#if !fleet.loaded}
          <div class="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">
            {#each Array(6) as _, i (i)}<div class="skeleton h-[196px] rounded-2xl"></div>{/each}
          </div>
        {:else if hosts.length === 0}
          <div class="card">
            <Empty icon={Sparkles} title="No hosts yet" body="Point a Monit agent at Monarch's collector and it will show up here within one poll cycle.">
              <a class="btn btn-primary" href="/settings/collector">Show me how</a>
            </Empty>
          </div>
        {:else}
          <div class="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">
            {#each hosts as h (h.id)}<HostCard host={h} />{/each}
          </div>
        {/if}
      </section>
    </div>

    <!-- right rail -->
    <aside class="min-w-0 space-y-6">
      <section class="card overflow-hidden">
        <div class="flex items-center justify-between border-b border-line px-4 py-3.5">
          <h2 class="card-title">Recent events</h2>
          <a href="/events" class="flex items-center gap-1 text-xs text-fg-3 hover:text-fg">View all <ArrowRight size={13} /></a>
        </div>
        {#if !data}
          <div class="space-y-3 p-4">{#each Array(6) as _, i (i)}<Skeleton class="h-10 w-full" />{/each}</div>
        {:else if recent.length === 0}
          <Empty icon={Activity} title="Quiet so far" body="Events from Monit will stream in here." />
        {:else}
          <div class="divide-y divide-[var(--line)]">
            {#each recent as e (e.id)}<div class="animate-in"><EventRow event={e} compact /></div>{/each}
          </div>
        {/if}
      </section>

      {#if data}
        <section class="card p-4">
          <div class="mb-3 flex items-center justify-between">
            <h2 class="card-title">Event activity</h2>
            <span class="text-[11px] text-fg-3">last 24 hours</span>
          </div>
          <div class="flex h-28 items-end gap-[3px]">
            {#each data.activity as a (a.ts)}
              {@const total = a.ok + a.failed}
              <div class="group relative flex h-full flex-1 flex-col justify-end" title="{timeShort(a.ts)} — {a.failed} failed, {a.ok} other">
                <div class="rounded-t-[3px] bg-[color-mix(in_oklab,var(--accent-2)_55%,transparent)] transition-opacity group-hover:opacity-100" style="height:{(a.ok / activityMax) * 100}%"></div>
                <div class="bg-bad {a.ok ? '' : 'rounded-t-[3px]'}" style="height:{(a.failed / activityMax) * 100}%"></div>
                {#if total === 0}<div class="h-px bg-[var(--line-strong)]"></div>{/if}
              </div>
            {/each}
          </div>
          <div class="num mt-2 flex justify-between text-[10px] text-fg-3">
            <span>{timeShort(data.activity[0]?.ts ?? 0)}</span><span>{timeShort(data.activity[12]?.ts ?? 0)}</span><span>now</span>
          </div>
        </section>
      {/if}
    </aside>
  </div>

  <!-- bottom row -->
  {#if data}
    <div class="grid gap-6 lg:grid-cols-3">
      <section class="card overflow-hidden">
        <div class="flex items-center gap-2 border-b border-line px-4 py-3.5">
          <Cpu size={15} class="text-accent" />
          <h2 class="card-title">Top CPU</h2>
        </div>
        <ul class="p-2">
          {#each data.top_cpu as p, i (p.host_id + p.service)}
            <li>
              <a href={serviceHref(p.host_id, p.service)} class="flex items-center gap-3 rounded-lg px-2.5 py-2 hover:bg-hover">
                <span class="num w-4 text-[11px] text-fg-3">{i + 1}</span>
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13px] font-medium text-fg">{p.service}</div>
                  <div class="truncate text-[11px] text-fg-3">{p.host}</div>
                </div>
                <div class="w-24"><UsageBar value={(p.cpu ?? 0) * (100 / Math.max(1, data.top_cpu[0]?.cpu ?? 1))} tone="ok" height={4} /></div>
                <span class="num w-14 text-right text-xs text-fg">{pct(p.cpu)}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>

      <section class="card overflow-hidden">
        <div class="flex items-center gap-2 border-b border-line px-4 py-3.5">
          <MemoryStick size={15} class="text-accent-2" />
          <h2 class="card-title">Top memory</h2>
        </div>
        <ul class="p-2">
          {#each data.top_mem as p, i (p.host_id + p.service)}
            <li>
              <a href={serviceHref(p.host_id, p.service)} class="flex items-center gap-3 rounded-lg px-2.5 py-2 hover:bg-hover">
                <span class="num w-4 text-[11px] text-fg-3">{i + 1}</span>
                <div class="min-w-0 flex-1">
                  <div class="truncate text-[13px] font-medium text-fg">{p.service}</div>
                  <div class="truncate text-[11px] text-fg-3">{p.host} · {pct(p.mem_percent)}</div>
                </div>
                <div class="w-24"><UsageBar value={((p.mem_kb ?? 0) / Math.max(1, data.top_mem[0]?.mem_kb ?? 1)) * 100} tone="info" height={4} /></div>
                <span class="num w-16 text-right text-xs text-fg">{kb(p.mem_kb)}</span>
              </a>
            </li>
          {/each}
        </ul>
      </section>

      <section class="card overflow-hidden">
        <div class="flex items-center gap-2 border-b border-line px-4 py-3.5">
          <HardDrive size={15} class="text-warn" />
          <h2 class="card-title">Filesystems</h2>
        </div>
        <ul class="space-y-1 p-2">
          {#each data.filesystems as f (f.host_id + f.service)}
            <li>
              <a href={serviceHref(f.host_id, f.service)} class="block rounded-lg px-2.5 py-2 hover:bg-hover">
                <div class="mb-1.5 flex items-center justify-between gap-3 text-[13px]">
                  <span class="min-w-0 truncate"><span class="font-medium text-fg">{f.service}</span> <span class="text-[11px] text-fg-3">{f.host}</span></span>
                  <span class="num text-xs tone-{usageTone(f.percent)} {f.percent >= 75 ? 'text-tone' : 'text-fg'}">{pct(f.percent)}</span>
                </div>
                <UsageBar value={f.percent} height={5} />
                <div class="num mt-1 text-[10px] text-fg-3">{mb(f.used_mb)} of {mb(f.total_mb)}</div>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    </div>
  {/if}
</div>

<style>
  .kpi::before {
    content: "";
    position: absolute;
    inset: 0;
    background: radial-gradient(120% 80% at 100% 0%, color-mix(in oklab, var(--kpi) 16%, transparent), transparent 60%);
    pointer-events: none;
  }
  .kpi-icon {
    display: inline-flex;
    height: 1.875rem;
    width: 1.875rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.625rem;
    color: var(--kpi);
    background: color-mix(in oklab, var(--kpi) 14%, transparent);
    border: 1px solid color-mix(in oklab, var(--kpi) 24%, transparent);
  }
</style>
