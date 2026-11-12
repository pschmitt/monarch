<script lang="ts">
  import { Search, Server } from "@lucide/svelte";
  import { ago, hostName, osLabel } from "../../lib/format";
  import { fuzzy } from "../../lib/fuzzy";
  import { clock, fleet } from "../../lib/state.svelte";
  import { router, hostHref } from "../../lib/router.svelte";
  import Hero from "../../lib/components/character/Hero.svelte";
  import Status from "../../lib/components/character/Status.svelte";

  const q = $derived(router.route.query.get("q") ?? "");
  const stateFilter = $derived(router.route.query.get("state") ?? "");
  const group = $derived(router.route.query.get("group") ?? "");

  const hosts = $derived(Object.values(fleet.hosts));
  const groups = $derived([...new Set(hosts.flatMap((h) => h.hostgroups))].sort());
  const rows = $derived.by(() => hosts
    .filter((h) => !stateFilter || (stateFilter === "online" ? h.online : h.state === stateFilter))
    .filter((h) => !group || h.hostgroups.includes(group))
    .map((h) => ({ host: h, score: q.trim() ? fuzzy(q, `${hostName(h)} ${h.hostname} ${osLabel(h.os)} ${h.hostgroups.join(" ")} ${h.description ?? ""}`) : 0 }))
    .filter((x) => x.score >= 0)
    .sort((a, b) => (a.host.state === "offline" ? 0 : a.host.state === "degraded" ? 1 : 2) - (b.host.state === "offline" ? 0 : b.host.state === "degraded" ? 1 : 2) || hostName(a.host).localeCompare(hostName(b.host), undefined, { numeric: true }))
    .map((x) => x.host));
  const queryString = $derived(router.route.query.toString());

  function updateFilter(key: "q" | "state" | "group", value: string) {
    router.setQuery({ q: key === "q" ? value : q, state: key === "state" ? value : stateFilter, group: key === "group" ? value : group });
  }
  function clearFilters() { router.setQuery({ q: null, state: null, group: null }); }
</script>

<div class="c-page">
  <Hero title="Hosts" description="Reporting agents, their health, and the checks they run." icon={Server}>
    <div class="c-stats">
      <a href="/hosts?state=ok"><strong>{hosts.filter((h) => h.state === "ok").length}</strong><span>healthy</span></a>
      <a href="/hosts?state=degraded"><strong>{hosts.filter((h) => h.state === "degraded").length}</strong><span>degraded</span></a>
      <a href="/hosts?state=offline"><strong>{hosts.filter((h) => h.state === "offline").length}</strong><span>offline</span></a>
    </div>
  </Hero>

  <section class="c-filters" aria-label="Filter hosts">
    <div class="relative min-w-56 flex-1 sm:max-w-xs"><Search size={15} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-fg-3" /><input class="input pl-9" value={q} oninput={(event) => updateFilter("q", (event.currentTarget as HTMLInputElement).value)} placeholder="Search hosts…" aria-label="Search hosts" /></div>
    <label class="grid gap-1 text-xs text-fg-3"><span>State</span><select class="input" value={stateFilter} onchange={(event) => updateFilter("state", (event.currentTarget as HTMLSelectElement).value)} aria-label="Filter by host state">
      <option value="">All states</option><option value="online">Online</option><option value="ok">Healthy</option><option value="degraded">Needs attention</option><option value="offline">Offline</option>
    </select></label>
    <label class="grid gap-1 text-xs text-fg-3"><span>Group</span><select class="input" value={group} onchange={(event) => updateFilter("group", (event.currentTarget as HTMLSelectElement).value)} aria-label="Filter by host group">
      <option value="">All groups</option>{#each groups as g (g)}<option value={g}>{g}</option>{/each}
    </select></label>
    {#if q || stateFilter || group}<button class="btn" onclick={clearFilters}>Clear filters</button>{/if}
    <span class="c-count" aria-live="polite">{rows.length} of {hosts.length} hosts</span>
  </section>

  {#if !fleet.loaded}
    <div class="c-card-list" aria-label="Loading hosts">{#each Array(6) as _, i (i)}<div class="c-row"><div class="skeleton h-10 w-full"></div></div>{/each}</div>
  {:else if rows.length === 0}
    <div class="card p-5 text-fg-3">No hosts match these filters.</div>
  {:else}
    <div class="c-card-list">
      {#each rows as h (h.id)}
        <a class="c-row c-host-row" href={`${hostHref(h.id)}${queryString ? `?${queryString}` : ""}`}>
          <span class="c-row-icon"><Server size={19} strokeWidth={1.7} /></span>
          <span class="c-row-main">
            <span class="c-row-title">{hostName(h)}{#if h.ignored} · Ignored{/if}</span>
            <span class="c-meta">{h.online ? `Seen ${ago(h.last_seen, clock.now)}` : `Last report ${ago(h.last_seen, clock.now)}`}</span>
            <span class="c-meta c-host-facts">Kernel {h.os.release ?? "—"} · Load avg {h.system.load ? h.system.load.map((v) => v.toFixed(2)).join("  ") : "—"}</span>
            <span class="c-meta">{h.services.total} services</span>
          </span>
          <Status state={h.state} />
          <span class="c-chevron" aria-hidden="true">›</span>
        </a>
      {/each}
    </div>
  {/if}
</div>
