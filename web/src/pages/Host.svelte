<script lang="ts">
  import { BellOff, ChevronRight, Clock, Cpu, MemoryStick, RefreshCw, ServerCrash, Tag } from "@lucide/svelte";
  import { api, ApiError } from "../lib/api";
  import type { HostDetail, ServiceCounts } from "../lib/types";
  import { ago, datetime, duration, hostName, hostStateLabel, hostTone, kb } from "../lib/format";
  import { clock, fleet, toastError } from "../lib/state.svelte";
  import { hostHref } from "../lib/router.svelte";
  import Badge from "../lib/components/Badge.svelte";
  import Empty from "../lib/components/Empty.svelte";
  import EventsList from "../lib/components/EventsList.svelte";
  import StatusDot from "../lib/components/StatusDot.svelte";
  import Tabs from "../lib/components/Tabs.svelte";
  import HostOverview from "./host/HostOverview.svelte";
  import HostServices from "./host/HostServices.svelte";
  import HostSettings from "./host/HostSettings.svelte";
  import { can } from "../lib/state.svelte";

  let { id, tab = "overview" }: { id: number; tab?: string } = $props();

  let host = $state<HostDetail | null>(null);
  let notFound = $state(false);
  let refreshing = $state(false);

  async function load() {
    refreshing = true;
    try {
      host = await api.host(id);
      notFound = false;
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) notFound = true;
      else toastError(e, "Failed to load host");
    } finally {
      refreshing = false;
    }
  }

  $effect(() => {
    void id;
    load();
  });

  // Refetch when a new report arrives for this host.
  const liveSeen = $derived(fleet.hosts[id]?.last_seen);
  $effect(() => {
    if (liveSeen && host && liveSeen > host.last_seen + 0.5) load();
  });

  const live = $derived(fleet.hosts[id]);
  const counts = $derived.by((): ServiceCounts => {
    const c = { total: 0, ok: 0, failed: 0, unmonitored: 0, pending: 0 };
    for (const s of host?.services ?? []) {
      c.total++;
      if (s.state === "ok") c.ok++;
      else if (s.state === "failed") c.failed++;
      else if (s.state === "unmonitored") c.unmonitored++;
      else c.pending++;
    }
    return c;
  });
  const hstate = $derived(live?.state ?? host?.state ?? "ok");
  const lastSeen = $derived(live?.last_seen ?? host?.last_seen ?? 0);
  const muted = $derived(!!host?.muted_until && host.muted_until > clock.now);

  $effect(() => {
    if (host) document.title = `${hostName(host)} · Monarch`;
  });

  const tabs = $derived([
    { id: "overview", label: "Overview" },
    { id: "services", label: "Services", count: counts.total },
    { id: "events", label: "Events" },
    ...(can("admin") ? [{ id: "settings", label: "Settings" }] : []),
  ]);
</script>

{#if notFound}
  <div class="card"><Empty icon={ServerCrash} title="Host not found" body="It may have been removed."><a class="btn btn-primary" href="/hosts">All hosts</a></Empty></div>
{:else if !host}
  <div class="space-y-4">
    <div class="skeleton h-5 w-40"></div>
    <div class="skeleton h-32 w-full rounded-2xl"></div>
    <div class="grid gap-4 lg:grid-cols-3">{#each Array(3) as _, i (i)}<div class="skeleton h-60 rounded-2xl"></div>{/each}</div>
  </div>
{:else}
  <div class="space-y-6">
    <nav class="flex items-center gap-1.5 text-xs text-fg-3" aria-label="Breadcrumb">
      <a href="/hosts" class="hover:text-fg">Hosts</a>
      <ChevronRight size={12} />
      <span class="text-fg-2">{hostName(host)}</span>
    </nav>

    <!-- hero -->
    <section class="card tone-{hostTone(hstate)} relative overflow-hidden p-5 sm:p-6">
      <div class="pointer-events-none absolute -top-24 -right-24 h-64 w-64 rounded-full bg-tone opacity-[0.10] blur-3xl"></div>
      <div class="relative flex flex-wrap items-start justify-between gap-5">
        <div class="min-w-0">
          <div class="flex flex-wrap items-center gap-3">
            <StatusDot tone={hostTone(hstate)} pulse={hstate !== "ok"} size={10} />
            <h1 class="truncate text-2xl font-semibold tracking-tight text-fg sm:text-[28px]">{hostName(host)}</h1>
            <Badge tone={hostTone(hstate)}>{hostStateLabel[hstate]}</Badge>
            {#if muted}<Badge tone="muted"><BellOff size={12} /> muted until {datetime(host.muted_until)}</Badge>{/if}
          </div>
          {#if host.display_name && host.display_name !== host.hostname}<div class="num mt-1 text-xs text-fg-3">{host.hostname}</div>{/if}
          {#if host.description}<p class="mt-2 max-w-2xl text-[13px] text-fg-2">{host.description}</p>{/if}
          <div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-fg-2">
            <span>{host.os.name} {host.os.release}</span>
            <span class="text-fg-3">{host.os.machine}</span>
            {#if host.cpu_count}<span class="flex items-center gap-1"><Cpu size={12} class="text-fg-3" />{host.cpu_count} CPU</span>{/if}
            {#if host.mem_total_kb}<span class="flex items-center gap-1"><MemoryStick size={12} class="text-fg-3" />{kb(host.mem_total_kb, 0)}</span>{/if}
            <span class="flex items-center gap-1"><Clock size={12} class="text-fg-3" />up {duration(live?.system.uptime ?? host.system.uptime)}</span>
            {#each host.hostgroups as g (g)}
              <a href="/hosts?group={encodeURIComponent(g)}" class="flex items-center gap-1 rounded-md border border-line px-1.5 py-px text-[11px] text-fg-3 hover:text-fg"><Tag size={10} />{g}</a>
            {/each}
          </div>
        </div>
        <div class="flex items-center gap-3">
          <div class="text-right">
            <div class="eyebrow">Last report</div>
            <div class="num mt-0.5 text-sm font-medium {host.online ? 'text-fg' : 'text-bad'}" title={datetime(lastSeen)}>{ago(lastSeen, clock.now)}</div>
            <div class="num text-[11px] text-fg-3">every {host.poll}s · Monit {host.monit_version ?? "?"}</div>
          </div>
          <button class="btn btn-icon" onclick={load} aria-label="Refresh" disabled={refreshing}><RefreshCw size={15} class={refreshing ? "animate-spin" : ""} /></button>
        </div>
      </div>

      <div class="relative mt-5 grid grid-cols-2 gap-3 sm:grid-cols-5">
        {#each [["Services", counts.total, "fg"], ["Healthy", counts.ok, "ok"], ["Failed", counts.failed, "bad"], ["Pending", counts.pending, "warn"], ["Unmonitored", counts.unmonitored, "muted"]] as [label, n, t] (label)}
          <a href={hostHref(host.id, "services") + (t === "fg" ? "" : `?state=${t === "bad" ? "failed" : t === "warn" ? "pending" : t === "muted" ? "unmonitored" : "ok"}`)} class="{t === 'fg' ? 'col-span-2 sm:col-span-1' : ''} rounded-xl border border-line bg-surface-2/50 px-3.5 py-2.5 transition-colors hover:border-line-strong tone-{t === 'fg' ? 'info' : t}">
            <div class="text-[11px] text-fg-3">{label}</div>
            <div class="num mt-0.5 text-xl font-semibold {t !== 'fg' && (n as number) > 0 ? 'text-tone' : 'text-fg'}">{n}</div>
          </a>
        {/each}
      </div>
    </section>

    <Tabs {tabs} active={tab} href={(t) => hostHref(host!.id, t === "overview" ? undefined : t)} />

    {#if tab === "services"}
      <HostServices {host} onchange={load} />
    {:else if tab === "events"}
      <EventsList host={host.id} />
    {:else if tab === "settings" && can("admin")}
      <HostSettings {host} onsaved={(h) => (host = h)} />
    {:else}
      <HostOverview {host} />
    {/if}
  </div>
{/if}
