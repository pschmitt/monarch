<script lang="ts">
  import { Activity, ArrowRight, ChevronRight, Plus, CircleCheckBig, Cpu, EyeOff, HardDrive, MemoryStick, Server, Siren, Sparkles } from "@lucide/svelte";
  import { api } from "../lib/api";
  import type { MonarchEvent, Overview } from "../lib/types";
  import { ago, hostName, kb, mb, pct, serviceTypeLabel, usageTone } from "../lib/format";
  import { clock, fleet, hostList, toastError, ui } from "../lib/state.svelte";
  import { hostHref, serviceHref } from "../lib/router.svelte";
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

  // Offline hosts head "Needs attention"; their services are left out of the
  // overview because they only hold the state of the last report.
  const offlineHosts = $derived(hosts.filter((h) => !h.ignored && h.state === "offline").sort((a, b) => a.last_seen - b.last_seen));
  const attentionCount = $derived((data?.failing.length ?? 0) + offlineHosts.length);

  // Live hosts take precedence over the (possibly stale) overview snapshot.
  const hostCounts = $derived.by(() => {
    const list = Object.values(fleet.hosts).filter((h) => !h.ignored);
    if (!Object.keys(fleet.hosts).length && data) return data.hosts;
    return {
      total: list.length,
      online: list.filter((h) => h.online).length,
      offline: list.filter((h) => !h.online).length,
      degraded: list.filter((h) => h.state === "degraded").length,
    };
  });

  const hostHealthSegments = $derived([
    { label: "Healthy", count: Math.max(0, hostCounts.total - hostCounts.degraded - hostCounts.offline), color: "var(--mn-ok)" },
    { label: "Degraded", count: hostCounts.degraded, color: "var(--mn-warn)" },
    { label: "Offline", count: hostCounts.offline, color: "var(--mn-bad)" },
  ]);

  const recent = $derived.by((): MonarchEvent[] => {
    const seen = new Set<number>();
    const out: MonarchEvent[] = [];
    for (const e of [...fleet.live, ...(data?.recent_events ?? [])]) {
      if (seen.has(e.id)) continue;
      seen.add(e.id);
      out.push(e);
    }
    return out.sort((a, b) => b.created_at - a.created_at).slice(0, 10);
  });

  const activityMax = $derived(Math.max(1, ...(data?.activity ?? []).map((a) => a.ok + a.failed)));
  const healthPct = $derived(data && data.services.total ? (data.services.ok / data.services.total) * 100 : null);
  const failingHostCount = $derived(data ? new Set(data.failing.map((f) => f.host_id)).size : 0);
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
        {:else if data.services.failed === 0 && data.services.unmonitored === 0 && hostCounts.offline === 0}
          Everything is <span class="text-gradient">running smoothly</span>
        {:else}
          {@const attention = data.services.failed + data.services.unmonitored + hostCounts.offline}
          <span class="text-bad">{attention}</span> thing{attention === 1 ? "" : "s"} need{attention === 1 ? "s" : ""} your attention
        {/if}
      </h1>
    </div>
    <div class="num text-[11px] text-fg-3">{loadedAt ? `updated ${ago(loadedAt, clock.now)}` : ""}</div>
  </div>

  <!-- KPI tiles -->
  <div class="overview-summary grid auto-rows-fr grid-cols-2 gap-3 xl:grid-cols-4 xl:gap-4">
    <a href="/hosts" class="summary-card card card-hover flex min-h-[144px] flex-col p-3 sm:p-4" aria-label="Hosts: {hostCounts.online} online of {hostCounts.total}, {hostCounts.degraded} degraded, {hostCounts.offline} offline">
      <div class="flex items-center gap-2 text-[13px] font-medium text-fg-2"><Server size={15} class="shrink-0 text-fg-3" />Hosts</div>
      <div class="mt-2">
        <div class="flex items-baseline gap-1.5"><span class="num text-2xl font-semibold leading-none tracking-tight text-fg">{hostCounts.online}</span><span class="num text-xs text-fg-3">/ {hostCounts.total}</span></div>
        <div class="mt-1 text-xs leading-4 text-fg-3">online</div>
      </div>
      <div class="mt-2 flex h-1.5 overflow-hidden rounded-full bg-line-strong" role="img" aria-label="Host health: {hostHealthSegments.map((s) => `${s.count} ${s.label.toLowerCase()}`).join(', ')}">
        {#each hostHealthSegments as segment (segment.label)}
          <span style="width: {hostCounts.total ? segment.count / hostCounts.total * 100 : 0}%; background: {segment.color}" title="{segment.label}: {segment.count}"></span>
        {/each}
      </div>
      <div class="mt-auto flex flex-wrap gap-x-3 pt-2 text-xs leading-4 text-fg-3">
        <span class={hostCounts.degraded ? "text-warn" : ""}>{hostCounts.degraded} degraded</span>
        <span class={hostCounts.offline ? "text-bad" : ""}>{hostCounts.offline} offline</span>
      </div>
    </a>

    <a href="/services" class="summary-card card card-hover flex min-h-[144px] flex-col p-3 sm:p-4" aria-label="Services: {data?.services.ok ?? 0} of {data?.services.total ?? 0} healthy">
      <div class="flex items-center gap-2 text-[13px] font-medium text-fg-2"><CircleCheckBig size={15} class="shrink-0 text-fg-3" />Services</div>
      {#if data}
        <div class="mt-2">
          <div class="flex items-baseline gap-1.5"><span class="num text-2xl font-semibold leading-none tracking-tight text-fg">{data.services.ok}</span><span class="num text-xs text-fg-3">/ {data.services.total}</span></div>
          <div class="mt-1 text-xs leading-4 text-fg-3">healthy</div>
        </div>
        <div class="mt-2"><UsageBar value={healthPct} tone={data.services.failed ? "warn" : "ok"} height={5} /></div>
        <div class="mt-auto flex flex-wrap gap-x-3 pt-2 text-xs leading-4 text-fg-3">
          {#if data.services.pending > 0}<span>{data.services.pending} pending</span>{/if}
          {#if data.services.unmonitored > 0}<span class="text-warn">{data.services.unmonitored} unmonitored</span>{/if}
          {#if data.services.pending === 0 && data.services.unmonitored === 0}<span>All monitored</span>{/if}
        </div>
      {:else}<Skeleton class="mt-2 h-7 w-24" /><Skeleton class="mt-2 h-1.5 w-full" /><span class="mt-auto pt-2 text-xs text-fg-3">Loading services</span>{/if}
    </a>

    <a href="/services?state=failed" class="summary-card card card-hover flex min-h-[144px] flex-col p-3 sm:p-4" aria-label="Failing services: {data?.services.failed ?? 0} across {failingHostCount} hosts">
      <div class="flex items-center gap-2 text-[13px] font-medium text-fg-2"><Siren size={15} class="shrink-0 text-fg-3" />Failing</div>
      {#if data}
        <div class="mt-2">
          <div class="num text-2xl font-semibold leading-none tracking-tight {data.services.failed ? 'text-bad' : 'text-fg'}">{data.services.failed}</div>
          <div class="mt-1 text-xs leading-4 text-fg-3">service{data.services.failed === 1 ? "" : "s"}</div>
        </div>
        <div class="mt-auto pt-2 text-xs leading-4 text-fg-3">{data.services.failed ? `Across ${failingHostCount} host${failingHostCount === 1 ? "" : "s"}` : "No failing services"}</div>
      {:else}<Skeleton class="mt-2 h-7 w-16" /><span class="mt-auto pt-2 text-xs text-fg-3">Loading status</span>{/if}
    </a>

    <a href="/events" class="summary-card card card-hover flex min-h-[144px] flex-col p-3 sm:p-4" aria-label="Events in the last 24 hours: {data?.events_24h ?? 0}">
      <div class="flex items-center gap-2 text-[13px] font-medium text-fg-2"><Activity size={15} class="shrink-0 text-fg-3" />Events</div>
      {#if data}
        <div class="mt-2">
          <div class="flex min-w-0 items-end justify-between gap-2">
            <span class="num shrink-0 text-2xl font-semibold leading-none tracking-tight text-fg">{data.events_24h}</span>
            <svg viewBox="0 0 96 28" class="h-12 w-32 min-w-0 max-w-[55%]" aria-hidden="true">
              {#each data.activity as a, i (a.ts)}
                {@const height = (a.ok + a.failed) / activityMax * 26}
                {@const failedHeight = a.failed / activityMax * 26}
                <rect x={i * 4} y={28 - height} width="3" height={height - failedHeight} rx="1" fill="var(--mn-accent-2)" opacity="0.6" />
                <rect x={i * 4} y={28 - failedHeight} width="3" height={failedHeight} rx="1" fill="var(--mn-bad)" />
              {/each}
            </svg>
          </div>
          <div class="mt-1 text-xs leading-4 text-fg-3">last 24 hours</div>
        </div>
        <div class="mt-auto pt-2 text-xs leading-4 text-fg-3">
          <div class="text-bad">{data.activity.reduce((s, a) => s + a.failed, 0)} failures</div>
        </div>
      {:else}<Skeleton class="mt-2 h-7 w-20" /><span class="mt-auto pt-2 text-xs text-fg-3">Loading events</span>{/if}
    </a>
  </div>

  <div class="overview-details-grid grid min-h-[240px] gap-6 xl:grid-cols-2 xl:gap-x-4">
    <div class="min-w-0 space-y-6">
      <!-- needs attention -->
      {#if data && attentionCount}
        <section class="card tone-bad overflow-hidden">
          <div class="flex items-center justify-between border-b border-line px-5 py-3.5">
            <div class="flex items-center gap-2.5">
              <StatusDot tone="bad" pulse />
              <h2 class="card-title">Needs attention</h2>
              <span class="num rounded-md bg-tone-soft px-1.5 text-[11px] font-semibold text-tone">{attentionCount}</span>
            </div>
          </div>
          <ul class="divide-y divide-[var(--mn-line)]">
            {#each offlineHosts as h (h.id)}
              <li>
                <a href={hostHref(h.id)} class="group flex items-center gap-3 px-5 py-3 transition-colors hover:bg-hover">
                  <span class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-tone-soft text-tone"><Server size={15} /></span>
                  <div class="min-w-0 flex-1">
                    <div class="truncate text-[13px] font-semibold text-fg">{hostName(h)}</div>
                    <div class="truncate text-xs text-tone">Offline <span class="text-fg-3">· Host</span></div>
                  </div>
                  <div class="num text-right text-[11px] text-fg-3">last seen<br /><span class="text-fg-2">{ago(h.last_seen, clock.now)}</span></div>
                  <ArrowRight size={15} class="text-fg-3 transition-transform group-hover:translate-x-0.5" />
                </a>
              </li>
            {/each}
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

      {#if data && data.unmonitored.length}
        <section class="card tone-warn overflow-hidden">
          <div class="flex items-center justify-between border-b border-line px-5 py-3.5">
            <div class="flex items-center gap-2.5">
              <EyeOff size={15} class="text-warn" />
              <h2 class="card-title">Unmonitored services</h2>
              <span class="num rounded-md bg-tone-soft px-1.5 text-[11px] font-semibold text-tone">{data.unmonitored.length}</span>
            </div>
            <a href="/services?state=unmonitored" class="flex items-center gap-1 text-xs text-fg-3 hover:text-fg">View all <ArrowRight size={13} /></a>
          </div>
          <ul class="divide-y divide-[var(--mn-line)]">
            {#each data.unmonitored as s (`${s.host_id}/${s.service}`)}
              <li>
                <a href={serviceHref(s.host_id, s.service)} class="group flex items-center gap-3 px-5 py-3 transition-colors hover:bg-hover">
                  <span class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-tone-soft text-tone"><ServiceIcon type={s.type} size={15} /></span>
                  <div class="min-w-0 flex-1">
                    <div class="truncate text-[13px]"><span class="font-semibold text-fg">{s.service}</span> <span class="text-fg-3">on</span> <span class="text-fg-2">{s.host}</span></div>
                    <div class="truncate text-xs text-tone">{s.status_text} <span class="text-fg-3">· {serviceTypeLabel[s.type]}</span></div>
                  </div>
                  {#if s.since}<span class="num shrink-0 text-xs text-fg-3">{ago(s.since, clock.now).replace(" ago", "")}</span>{/if}
                  <ArrowRight size={15} class="text-fg-3 transition-transform group-hover:translate-x-0.5" />
                </a>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
      {#if data && !attentionCount && !data.unmonitored.length}
        <section class="card min-h-[240px]"><Empty icon={CircleCheckBig} title="No active failures" body="New problems will appear here." /></section>
      {/if}
    </div>

    <!-- right rail -->
    <aside class="recent-events-column min-w-0 xl:relative xl:min-h-0">
      <section class="recent-events-card card flex max-h-[32rem] flex-col overflow-hidden xl:absolute xl:inset-0 xl:max-h-none">
        <div class="flex shrink-0 items-center justify-between border-b border-line px-4 py-3.5">
          <h2 class="card-title">Recent events</h2>
          <a href="/events" class="flex items-center gap-1 text-xs text-fg-3 hover:text-fg">View all <ArrowRight size={13} /></a>
        </div>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard users need to scroll this region) -->
        <div class="recent-events-scroll min-h-0 flex-1 overflow-y-auto" role="region" aria-label="Recent events" tabindex="0">
        {#if !data}
          <div class="space-y-3 p-4">{#each Array(6) as _, i (i)}<Skeleton class="h-10 w-full" />{/each}</div>
        {:else if recent.length === 0}
          <Empty icon={Activity} title="Quiet so far" body="Events from Monit will stream in here." />
        {:else}
          <div class="divide-y divide-[var(--mn-line)]">
            {#each recent as e (e.id)}<div class="animate-in"><EventRow event={e} compact /></div>{/each}
          </div>
        {/if}
        </div>
      </section>


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
          {#each data.top_cpu.slice(0, 5) as p, i (p.host_id + p.service)}
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
          {#each data.top_mem.slice(0, 5) as p, i (p.host_id + p.service)}
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
          {#each data.filesystems.slice(0, 5) as f (f.host_id + f.service)}
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
      <!-- fleet -->
      <section>
        <details class="fleet-disclosure group">
        <summary class="mb-3 flex cursor-pointer items-center justify-between gap-3">
          <span class="flex items-center gap-2"><ChevronRight size={16} class="transition-transform group-open:rotate-90" /><span class="card-title">Fleet</span><span class="text-xs text-fg-3">{hosts.length} hosts</span></span>
          <a href="/hosts" class="flex items-center gap-1 text-xs text-fg-3 hover:text-fg">All hosts <ArrowRight size={13} /></a>
        </summary>
        {#if !fleet.loaded}
          <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
            {#each Array(6) as _, i (i)}<div class="skeleton h-[196px] rounded-2xl"></div>{/each}
          </div>
        {:else if hosts.length === 0}
          <div class="card">
            <Empty icon={Sparkles} title="No hosts yet" body="Point a Monit agent at Monarch's collector and it will show up here within one poll cycle.">
              <button class="btn btn-primary" onclick={() => (ui.addHost = true)}><Plus size={15} /> Add a host</button>
            </Empty>
          </div>
        {:else}
          <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
            {#each hosts as h (h.id)}<HostCard host={h} />{/each}
          </div>
        {/if}
        </details>
      </section>

</div>

<style>
  .fleet-disclosure > summary { list-style: none; }
  .fleet-disclosure > summary::-webkit-details-marker { display: none; }
</style>
