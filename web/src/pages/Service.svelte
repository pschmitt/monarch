<script lang="ts">
  import { Bell, Braces, ChevronRight, Eye, EyeOff, Play, RotateCw, SearchX, Square } from "@lucide/svelte";
  import { api, ApiError } from "../lib/api";
  import type { ServiceAction, ServiceDetail } from "../lib/types";
  import { ago, datetime, hostName, serviceTone, serviceTypeLabel } from "../lib/format";
  import { ACTIONS, runAction } from "../lib/actions";
  import { chartsFor } from "../lib/service";
  import { can, clock, fleet, toastError } from "../lib/state.svelte";
  import { hostHref } from "../lib/router.svelte";
  import Badge from "../lib/components/Badge.svelte";
  import CheckAlerts from "../lib/components/CheckAlerts.svelte";
  import Empty from "../lib/components/Empty.svelte";
  import EventsList from "../lib/components/EventsList.svelte";
  import MetricChart from "../lib/components/MetricChart.svelte";
  import RangePicker from "../lib/components/RangePicker.svelte";
  import ServiceIcon from "../lib/components/ServiceIcon.svelte";
  import ServiceDetails from "./service/ServiceDetails.svelte";

  let { hostId, name }: { hostId: number; name: string } = $props();

  let svc = $state<ServiceDetail | null>(null);
  let alertsOpen = $state(false);
  let notFound = $state(false);
  let showRaw = $state(false);
  let range = $state(localStorage.getItem("monarch.range") ?? "6h");

  async function load() {
    try {
      svc = await api.service(hostId, name);
      notFound = false;
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) notFound = true;
      else if (!svc) notFound = true;
      else toastError(e, "Failed to refresh service");
    }
  }

  $effect(() => {
    void hostId;
    void name;
    load();
  });

  const liveSeen = $derived(fleet.hosts[hostId]?.last_seen);
  $effect(() => {
    if (liveSeen && svc && liveSeen > svc.host.last_seen + 0.5) load();
  });

  $effect(() => {
    if (svc) document.title = `${svc.name} · ${hostName(svc.host)} · Monarch`;
  });

  const icons: Record<ServiceAction, any> = { start: Play, stop: Square, restart: RotateCw, monitor: Eye, unmonitor: EyeOff };
  const canAct = $derived(!!svc && can("operator") && svc.host.can_act);
  const charts = $derived(svc ? chartsFor(svc) : []);

  async function act(a: ServiceAction) {
    if (!svc) return;
    if (await runAction(hostId, hostName(svc.host), [svc.name], a)) setTimeout(load, 1500);
  }
</script>

{#if notFound}
  <div class="card"><Empty icon={SearchX} title="Service not found" body="It may have been removed from the Monit configuration."><a class="btn btn-primary" href={hostHref(hostId, "services")}>Back to host</a></Empty></div>
{:else if !svc}
  <div class="space-y-4"><div class="skeleton h-5 w-56"></div><div class="skeleton h-36 rounded-2xl"></div><div class="grid gap-4 lg:grid-cols-2"><div class="skeleton h-64 rounded-2xl"></div><div class="skeleton h-64 rounded-2xl"></div></div></div>
{:else}
  {@const tone = serviceTone(svc.state)}
  <div class="space-y-6">
    <nav class="flex items-center gap-1.5 text-xs text-fg-3" aria-label="Breadcrumb">
      <a href="/hosts" class="hover:text-fg">Hosts</a>
      <ChevronRight size={12} />
      <a href={hostHref(hostId)} class="hover:text-fg">{hostName(svc.host)}</a>
      <ChevronRight size={12} />
      <a href={hostHref(hostId, "services")} class="hover:text-fg">Services</a>
      <ChevronRight size={12} />
      <span class="truncate text-fg-2">{svc.name}</span>
    </nav>

    <section class="card tone-{tone} relative overflow-hidden p-5 sm:p-6">
      <div class="pointer-events-none absolute -top-24 -left-16 h-64 w-64 rounded-full bg-tone opacity-[0.10] blur-3xl"></div>
      <div class="relative flex flex-wrap items-start justify-between gap-5">
        <div class="flex min-w-0 items-start gap-4">
          <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl border border-tone-soft bg-tone-soft text-tone">
            <ServiceIcon type={svc.type} size={22} />
          </div>
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2.5">
              <h1 class="truncate text-2xl font-semibold tracking-tight text-fg">{svc.name}</h1>
              <Badge {tone} dot pulse={svc.state === "failed"}>{svc.status_text}</Badge>
            </div>
            <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-fg-3">
              <span>{serviceTypeLabel[svc.type]}</span>
              <span>on <a class="text-fg-2 hover:underline" href={hostHref(hostId)}>{hostName(svc.host)}</a></span>
              {#if svc.state_since}<span title={datetime(svc.state_since)}>{svc.state === "ok" ? "healthy" : "in this state"} for {ago(svc.state_since, clock.now).replace(" ago", "")}</span>{/if}
              <span>collected {ago(svc.collected_at, clock.now)}</span>
              <span>{svc.monitor_mode} mode</span>
              {#if svc.every}<span>{svc.every}</span>{/if}
              {#if svc.pending_action}<span class="text-warn">{svc.pending_action} pending</span>{/if}
              {#each svc.groups as g (g)}<span class="rounded-md border border-line px-1.5 text-[11px]">{g}</span>{/each}
            </div>
          </div>
        </div>
        <div class="flex flex-wrap gap-2">
          {#if canAct}
            {#each ACTIONS as a (a.id)}
              {@const Icon = icons[a.id]}
              {#if !(a.id === "monitor" && svc.state !== "unmonitored") && !(a.id === "unmonitor" && svc.state === "unmonitored")}
                <button class="btn btn-sm {a.danger ? 'hover:text-bad' : ''}" onclick={() => act(a.id)}><Icon size={13} />{a.label}</button>
              {/if}
            {/each}
          {/if}
          {#if can("admin")}
            <button class="btn btn-sm" onclick={() => (alertsOpen = true)}><Bell size={13} />Alerts</button>
          {/if}
        </div>
      </div>
    </section>

    <ServiceDetails service={svc} />

    {#if charts.length}
      <div class="flex flex-wrap items-center justify-between gap-3">
        <h2 class="card-title">History</h2>
        <RangePicker bind:value={range} />
      </div>
      <div class="grid gap-4 lg:grid-cols-2">
        {#each charts as c (c.title)}
          <MetricChart host={hostId} service={svc.name} title={c.title} metrics={c.metrics} format={c.format} stacked={c.stacked} yMax={c.yMax ?? null} {range} />
        {/each}
      </div>
    {/if}

    <div>
      <h2 class="mb-3 card-title">Events</h2>
      <EventsList host={hostId} service={svc.name} filters={false} pageSize={25} />
    </div>

    <section class="card overflow-hidden">
      <button class="flex w-full items-center gap-2 px-5 py-3.5 text-left text-[13px] font-medium text-fg-2 hover:text-fg" onclick={() => (showRaw = !showRaw)} aria-expanded={showRaw}>
        <Braces size={15} /> Raw data
        <ChevronRight size={14} class="ml-auto transition-transform {showRaw ? 'rotate-90' : ''}" />
      </button>
      {#if showRaw}
        <pre class="num max-h-[480px] overflow-auto border-t border-line bg-[var(--glass)] p-5 text-[11.5px] leading-relaxed text-fg-2">{JSON.stringify({ ...svc, host: undefined, recent_events: undefined }, null, 2)}</pre>
      {/if}
    </section>
  </div>
{/if}

{#if svc && can("admin")}
  <CheckAlerts bind:open={alertsOpen} {hostId} service={svc.name} hostName={hostName(svc.host)} />
{/if}
