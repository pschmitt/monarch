<script lang="ts">
  import { Search, ListChecks } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { HostDetail, Service, ServiceListItem, ServiceType } from "../../lib/types";
  import { hostName, mb, serviceTypeLabel, serviceTypeOrder } from "../../lib/format";
  import { keyMetrics } from "../../lib/service";
  import { fleet, toastError } from "../../lib/state.svelte";
  import { router, serviceHref } from "../../lib/router.svelte";
  import Hero from "../../lib/components/character/Hero.svelte";
  import Status from "../../lib/components/character/Status.svelte";
  import ServiceIcon from "../../lib/components/ServiceIcon.svelte";
  import UsageBar from "../../lib/components/UsageBar.svelte";

  const q = $derived(router.route.query.get("q") ?? "");
  const stateFilter = $derived(router.route.query.get("state") ?? "");
  const hostFilter = $derived(router.route.query.get("host") ?? "");
  const typeFilter = $derived(router.route.query.get("type") ?? "");
  const page = $derived(Math.max(1, Number(router.route.query.get("page") ?? 1) || 1));
  let services = $state<ServiceListItem[] | null>(null);
  let counts = $state<Record<string, number>>({});
  let serviceData = $state<Record<string, Service>>({});
  const pageSize = 20;
  let loadGeneration = 0;
  let detailGeneration = 0;
  const hostDetailCache = new Map<number, { lastSeen: number; promise: Promise<HostDetail | null> }>();

  const liveSeen = $derived(Object.values(fleet.hosts).reduce((latest, h) => Math.max(latest, h.last_seen), 0));
  $effect(() => {
    const query = q.trim();
    const state = stateFilter || null;
    const host = hostFilter ? Number(hostFilter) : null;
    void liveSeen;
    const generation = ++loadGeneration;
    const timer = setTimeout(async () => {
      try {
        const result = await api.services({ q: query || null, state, host });
        if (generation === loadGeneration) {
          services = result.services;
          counts = Object.fromEntries(Object.entries(result.counts).map(([key, value]) => [key, value ?? 0]));
          if (page > Math.ceil((result.services.length || 1) / pageSize)) router.setQuery({ page: null });
        }
      } catch (error) {
        toastError(error, "Failed to load services");
      }
    }, query ? 220 : 0);
    return () => clearTimeout(timer);
  });

  const filtered = $derived((services ?? []).filter((s) => !typeFilter || s.type === typeFilter));
  const pageCount = $derived(Math.max(1, Math.ceil(filtered.length / pageSize)));
  const pageItems = $derived(filtered.slice((page - 1) * pageSize, page * pageSize));
  const hosts = $derived(Object.values(fleet.hosts).sort((a, b) => hostName(a).localeCompare(hostName(b), undefined, { numeric: true })));
  const queryString = $derived(router.route.query.toString());

  $effect(() => {
    const visible = pageItems;
    const liveHosts = fleet.hosts;
    const generation = ++detailGeneration;
    let active = true;
    const ids = [...new Set(visible.map((s) => s.host_id))];
    Promise.all(ids.map(async (id) => {
      const lastSeen = liveHosts[id]?.last_seen ?? 0;
      let cached = hostDetailCache.get(id);
      if (!cached || cached.lastSeen !== lastSeen) {
        const promise = api.host(id).catch(() => null);
        cached = { lastSeen, promise };
        hostDetailCache.set(id, cached);
      }
      return [id, await cached.promise] as const;
    })).then((entries) => {
      if (!active || generation !== detailGeneration) return;
      const next: Record<string, Service> = {};
      for (const [hostId, detail] of entries) for (const service of detail?.services ?? []) next[`${hostId}/${service.name}`] = service;
      serviceData = next;
    });
    return () => { active = false; };
  });

  function updateFilter(kind: "q" | "state" | "host" | "type", value: string) {
    router.setQuery({
      q: kind === "q" ? value : q,
      state: kind === "state" ? value : stateFilter,
      host: kind === "host" ? value : hostFilter,
      type: kind === "type" ? value : typeFilter,
      page: null,
    });
  }

  function clearFilters() {
    router.setQuery({ q: null, state: null, host: null, type: null, page: null });
  }

  function typeText(type: ServiceType): string {
    return serviceTypeLabel[type];
  }
</script>

<div class="c-page">
  <Hero title="Services" description="Every monitored check across the fleet, with failures first." icon={ListChecks}>
    <div class="c-stats">
      <a href="/services?state=failed"><strong>{counts.failed ?? "—"}</strong><span class="mt-1 block text-sm text-fg-3">failed</span></a>
      <a href="/services?state=ok"><strong>{counts.ok ?? "—"}</strong><span class="mt-1 block text-sm text-fg-3">healthy</span></a>
      <a href="/services?state=stale"><strong>{counts.stale ?? "—"}</strong><span class="mt-1 block text-sm text-fg-3">stale</span></a>
    </div>
  </Hero>

  <section class="c-filters" aria-label="Filter services">
    <div class="relative min-w-56 flex-1 sm:max-w-xs"><Search size={15} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-fg-3" /><input class="input pl-9" value={q} oninput={(event) => updateFilter("q", (event.currentTarget as HTMLInputElement).value)} placeholder="Search services or hosts…" aria-label="Search services or hosts" /></div>
    <label class="grid gap-1 text-xs text-fg-3"><span>State</span><select class="input" value={stateFilter} onchange={(event) => updateFilter("state", (event.currentTarget as HTMLSelectElement).value)} aria-label="Filter by service state">
      <option value="">All states</option><option value="failed">Failed</option><option value="ok">Healthy</option><option value="pending">Pending</option><option value="unmonitored">Unmonitored</option><option value="stale">Stale</option>
    </select></label>
    <label class="grid gap-1 text-xs text-fg-3"><span>Host</span><select class="input" value={hostFilter} onchange={(event) => updateFilter("host", (event.currentTarget as HTMLSelectElement).value)} aria-label="Filter by host">
      <option value="">All hosts</option>{#each hosts as h (h.id)}<option value={h.id}>{hostName(h)}</option>{/each}
    </select></label>
    <label class="grid gap-1 text-xs text-fg-3"><span>Type</span><select class="input" value={typeFilter} onchange={(event) => updateFilter("type", (event.currentTarget as HTMLSelectElement).value)} aria-label="Filter by service type">
      <option value="">All types</option>{#each serviceTypeOrder as type (type)}<option value={type}>{typeText(type)}</option>{/each}
    </select></label>
    {#if q || stateFilter || hostFilter || typeFilter}<button class="btn" onclick={clearFilters}>Clear filters</button>{/if}
    <span class="c-count" aria-live="polite">{services ? `${filtered.length} services` : "Loading services…"}</span>
  </section>

  {#if !services}
    <div class="c-card-list" aria-label="Loading services">{#each Array(8) as _, i (i)}<div class="c-row"><div class="skeleton h-10 w-full"></div></div>{/each}</div>
  {:else if filtered.length === 0}
    <div class="card p-5 text-fg-3">No services match these filters.</div>
  {:else}
    <div class="c-card-list">
      {#each pageItems as item (`${item.host_id}/${item.name}`)}
        {@const detail = serviceData[`${item.host_id}/${item.name}`]}
        {@const metrics = detail ? keyMetrics(detail) : []}
        {@const space = detail?.type === "filesystem" ? detail.data.space : null}
        {@const detailLine = item.state === "ok" ? `${item.host} · ${typeText(item.type)}` : `${item.host} · ${typeText(item.type)} · ${item.status_text}`}
        <a class="c-row" href={`${serviceHref(item.host_id, item.name)}${queryString ? `?${queryString}` : ""}`} title={item.stale ? "Host offline; showing last reported state" : undefined}>
          <span class="c-row-icon"><ServiceIcon type={item.type} size={19} /></span>
          <span class="c-row-main">
            <span class="c-row-title">{item.name}</span>
            <span class="c-meta">{detailLine}</span>
            {#if metrics.length}
              <span class="flex flex-wrap gap-x-4 text-xs text-fg-3">{#each metrics as metric (metric.label)}<span><small class="text-fg-3">{metric.label}</small> {metric.value}</span>{/each}</span>
            {/if}
            {#if space?.percent != null}<UsageBar value={space.percent} height={5} /><span class="c-meta">{mb(space.used_mb)} of {mb(space.total_mb)} used</span>{/if}
          </span>
          <Status state={item.stale ? "stale" : item.state} label={item.stale ? "Stale" : undefined} />
          <span class="text-xl text-fg-3" aria-hidden="true">›</span>
        </a>
      {/each}
    </div>
    {#if pageCount > 1}
      <nav class="flex items-center justify-end gap-3 text-sm text-fg-3" aria-label="Service pages">
        <button class="btn" disabled={page <= 1} onclick={() => router.setQuery({ page: String(page - 1) })}>Previous</button>
        <span>Page {page} of {pageCount}</span>
        <button class="btn" disabled={page >= pageCount} onclick={() => router.setQuery({ page: String(page + 1) })}>Next</button>
      </nav>
    {/if}
  {/if}
</div>
