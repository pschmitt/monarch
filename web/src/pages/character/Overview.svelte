<script lang="ts">
  import { onMount } from "svelte";
  import { ArrowRight, Server, ServerOff } from "@lucide/svelte";
  import type { HostSummary, MonarchEvent, Overview, Service } from "../../lib/types";
  import { api } from "../../lib/api";
  import { ago, hostName } from "../../lib/format";
  import { keyMetrics } from "../../lib/service";
  import { fleet, onLiveEvent, toastError } from "../../lib/state.svelte";
  import { hostHref, serviceHref } from "../../lib/router.svelte";
  import Chart, { type ChartSeries } from "../../lib/components/Chart.svelte";
  import Hero from "../../lib/components/character/Hero.svelte";
  import Status from "../../lib/components/character/Status.svelte";
  import ServiceIcon from "../../lib/components/ServiceIcon.svelte";
  import Timeline from "../../lib/components/character/Timeline.svelte";

  let data = $state<Overview | null>(null);
  let events = $state<MonarchEvent[]>([]);
  let loading = $state(true);
  let attentionData = $state<Record<string, Service>>({});
  let generation = 0;
  let detailGeneration = 0;

  async function load() {
    const current = ++generation;
    try {
      const [overview, recent] = await Promise.all([api.overview(), api.events({ limit: 5 })]);
      if (current !== generation) return;
      data = overview;
      events = recent.events.slice(0, 5);
    } catch (error) {
      toastError(error, "Failed to load the fleet overview");
    } finally {
      if (current === generation) loading = false;
    }
  }

  onMount(() => {
    void load();
    const timer = setInterval(load, 30000);
    const unsubscribe = onLiveEvent(() => { void load(); });
    return () => { clearInterval(timer); unsubscribe(); };
  });

  const liveHosts = $derived(Object.values(fleet.hosts));
  const offlineHosts = $derived(liveHosts.filter((h) => !h.online).sort((a, b) => a.last_seen - b.last_seen));
  const attention = $derived.by(() => {
    const rows: ({ kind: "host"; host: HostSummary } | { kind: "service"; service: Overview["failing"][number] })[] = [];
    for (const host of offlineHosts) rows.push({ kind: "host", host });
    for (const service of data?.failing ?? []) rows.push({ kind: "service", service });
    return rows.slice(0, 3);
  });
  $effect(() => {
    const serviceItems = attention.filter((item): item is { kind: "service"; service: Overview["failing"][number] } => item.kind === "service");
    const current = ++detailGeneration;
    let active = true;
    const hostIds = [...new Set(serviceItems.map((item) => item.service.host_id))];
    Promise.all(hostIds.map(async (hostId) => {
      try { return [hostId, await api.host(hostId)] as const; }
      catch { return [hostId, null] as const; }
    })).then((rows) => {
      if (!active || current !== detailGeneration) return;
      const next: Record<string, Service> = {};
      for (const [hostId, host] of rows) for (const service of host?.services ?? []) next[`${hostId}/${service.name}`] = service;
      attentionData = next;
    });
    return () => { active = false; };
  });
  const chartX = $derived((data?.activity ?? []).map((p) => p.ts));
  const chartSeries = $derived<ChartSeries[]>([
    { label: "Changes", values: (data?.activity ?? []).map((p) => p.ok), color: "var(--mn-accent)" },
    { label: "Failures", values: (data?.activity ?? []).map((p) => p.failed), color: "var(--mn-bad)" },
  ]);
</script>

<div class="c-page">
  <Hero title="Fleet overview" description="Host health and service checks across your fleet." icon={Server}>
    {#if data}
    <div class="c-summary-links">
        <a href="/hosts?state=online">{data.hosts.online} online</a>
        <a href="/hosts?state=degraded">{data.hosts.degraded} degraded</a>
        <a href="/hosts?state=offline">{data.hosts.offline} offline</a>
        <a href="/services">{data.services.total} services</a>
      </div>
    {/if}
  </Hero>

  {#if loading && !data}
    <div class="c-card-list" aria-label="Loading overview">{#each Array(4) as _, i (i)}<div class="c-row"><div class="skeleton h-10 w-full"></div></div>{/each}</div>
  {:else if data}
    <section class="c-overview-attention">
      <div class="c-section-title flex items-center justify-between gap-3"><h2>Needs attention</h2><a href="/services?state=failed">All failing services <ArrowRight size={15} /></a></div>
      {#if attention.length}
        <div class="c-card-list">
          {#each attention as item, i (`${item.kind}-${i}`)}
            {#if item.kind === "host"}
              <a class="c-row" href={hostHref(item.host.id)}>
                <span class="c-row-icon"><ServerOff size={19} /></span>
                <span class="c-row-main"><span class="c-row-title">{hostName(item.host)}</span><span class="c-meta">Host offline · last report {ago(item.host.last_seen)}</span>{#if item.host.os.release || item.host.system.load}<span class="c-meta">Last reported: {#if item.host.os.release}Kernel {item.host.os.release}{/if}{#if item.host.os.release && item.host.system.load} · {/if}{#if item.host.system.load}Load avg {item.host.system.load.map((v) => v.toFixed(2)).join("  ")}{/if}</span>{/if}</span>
                <Status state="offline" />
                <span class="text-xl text-fg-3" aria-hidden="true">›</span>
              </a>
            {:else}
              {@const serviceData = attentionData[`${item.service.host_id}/${item.service.service}`]}
              {@const metrics = serviceData ? keyMetrics(serviceData).slice(0, 2) : []}
              <a class="c-row" href={serviceHref(item.service.host_id, item.service.service)}>
                <span class="c-row-icon"><ServiceIcon type={item.service.type} size={19} /></span>
                <span class="c-row-main"><span class="c-row-title">{item.service.service}</span><span class="c-meta">{item.service.host} · {item.service.status_text}</span>{#if metrics.length}<span class="c-meta">{#each metrics as metric, index (metric.label)}{#if index} · {/if}{metric.label} {metric.value}{/each}</span>{/if}</span>
                <Status state="failed" />
                <span class="text-xl text-fg-3" aria-hidden="true">›</span>
              </a>
            {/if}
          {/each}
        </div>
      {:else}
        <div class="card p-5 text-fg-3">No hosts or checks need attention.</div>
      {/if}
    </section>

    <section class="c-chart-section">
      <div class="c-section-title flex items-center justify-between gap-3"><h2>Activity · last 24 hours</h2></div>
      {#if chartX.length}
        <div class="c-chart"><div class="c-chart-key" aria-label="Chart legend"><span><i style="background:var(--mn-accent)"></i>Recoveries and changes</span><span><i style="background:var(--mn-bad)"></i>Failures</span></div><Chart x={chartX} series={chartSeries} span={86400} height={180} format={(v) => String(Math.round(v))} /></div>
      {:else}
        <div class="card p-5 text-fg-3">No activity history is available.</div>
      {/if}
    </section>

    <section class="c-recent-events">
      <div class="c-section-title flex items-center justify-between gap-3"><h2>Recent events</h2><a href="/events">All events <ArrowRight size={15} /></a></div>
      <Timeline {events} compact />
    </section>
  {/if}
</div>
