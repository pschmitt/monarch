<script lang="ts">
  import { Activity, Inbox, Search, X } from "@lucide/svelte";
  import Hero from "../../lib/components/character/Hero.svelte";
  import Timeline from "../../lib/components/character/Timeline.svelte";
  import { api } from "../../lib/api";
  import { hostName } from "../../lib/format";
  import type { MonarchEvent } from "../../lib/types";
  import { fleet, hostList, onLiveEvent, toastError } from "../../lib/state.svelte";
  import { router } from "../../lib/router.svelte";
  import Empty from "../../lib/components/Empty.svelte";

  const query = $derived(router.route.query.get("q") ?? "");
  const stateFilter = $derived(router.route.query.get("state") ?? "");
  const hostFilter = $derived(router.route.query.get("host") ?? "");
  const unacked = $derived(router.route.query.get("unacked") === "true");
  const hosts = $derived(hostList());

  let events = $state<MonarchEvent[]>([]);
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let generation = 0;
  let debounce: ReturnType<typeof setTimeout> | null = null;

  function setFilter(key: "q" | "state" | "host" | "unacked", value: string) {
    router.setQuery({ [key]: value });
  }

  function clearFilters() {
    router.setQuery({ q: null, state: null, host: null, unacked: null });
  }

  async function load(reset: boolean) {
    const requestGeneration = reset ? ++generation : generation;
    if (reset) loading = true;
    else loadingMore = true;
    try {
      const result = await api.events({
        q: query.trim() || null,
        state: stateFilter || null,
        host: hostFilter ? Number(hostFilter) : null,
        unacked,
        limit: 50,
        before: reset ? null : events.at(-1)?.id ?? null,
      });
      if (requestGeneration !== generation) return;
      events = reset ? result.events : [...events, ...result.events.filter((event) => !events.some((item) => item.id === event.id))];
      hasMore = result.has_more;
    } catch (error) {
      if (requestGeneration === generation) toastError(error, "Failed to load events");
    } finally {
      if (requestGeneration === generation) {
        loading = false;
        loadingMore = false;
      }
    }
  }

  $effect(() => {
    void query;
    void stateFilter;
    void hostFilter;
    void unacked;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => void load(true), query ? 250 : 0);
    return () => {
      if (debounce) clearTimeout(debounce);
    };
  });

  $effect(() => {
    const activeQuery = query.trim().toLowerCase();
    const activeState = stateFilter;
    const activeHost = hostFilter;
    const onlyUnacked = unacked;
    return onLiveEvent((event) => {
      if (activeHost && event.host_id !== Number(activeHost)) return;
      if (activeState && event.state !== activeState) return;
      if (onlyUnacked && event.acked_by) return;
      if (activeQuery && !`${event.host ?? ""} ${event.service ?? ""} ${event.kind_label} ${event.message}`.toLowerCase().includes(activeQuery)) return;
      if (!events.some((item) => item.id === event.id)) events = [event, ...events];
    });
  });

  const filtersActive = $derived(!!query || !!stateFilter || !!hostFilter || unacked);
</script>

<div class="c-page">
  <Hero title="Events" description="What changed across your fleet." icon={Activity}>
    <span class="c-live-note c-live-{fleet.status}"><span class="c-live-dot"></span>{fleet.status === "live" ? "Live updates" : fleet.status === "connecting" ? "Connecting to live updates" : "Live updates unavailable"}</span>
  </Hero>

  <div class="c-filters" aria-label="Filter events">
    <label class="c-search">
      <Search size={17} aria-hidden="true" />
      <input value={query} oninput={(event) => setFilter("q", event.currentTarget.value)} placeholder="Find an event…" aria-label="Search events" />
    </label>
    <select value={stateFilter} onchange={(event) => setFilter("state", event.currentTarget.value)} aria-label="Filter by state">
      <option value="">Any state</option>
      <option value="succeeded">Recovered</option>
      <option value="failed">Failed</option>
      <option value="changed">Changed</option>
      <option value="changed_not">Unchanged</option>
      <option value="init">Starting</option>
    </select>
    <select value={hostFilter} onchange={(event) => setFilter("host", event.currentTarget.value)} aria-label="Filter by host">
      <option value="">All hosts</option>
      {#each hosts as host (host.id)}<option value={host.id}>{hostName(host)}</option>{/each}
    </select>
    <label class="c-check-filter"><input type="checkbox" checked={unacked} onchange={(event) => setFilter("unacked", event.currentTarget.checked ? "true" : "")} /> Unacknowledged</label>
    {#if filtersActive}
      <button class="c-clear-filter" type="button" onclick={clearFilters}><X size={15} /> Clear</button>
    {/if}
  </div>

  <div class="c-event-count" aria-live="polite">
    {#if loading && events.length === 0}Loading events…
    {:else if hasMore}Showing {events.length} matching events so far
    {:else}{events.length} matching {events.length === 1 ? "event" : "events"}{/if}
  </div>

  {#if loading && events.length === 0}
    <div class="c-empty-panel" role="status">Loading events…</div>
  {:else if events.length === 0}
    <section class="c-empty-panel">
      <Empty icon={Inbox} title="No events" body={filtersActive ? "Nothing matches these filters." : "Monit hasn't reported any events yet."} />
    </section>
  {:else}
    <Timeline {events} />
    {#if hasMore}
      <div class="c-load-more">
        <button type="button" class="btn" disabled={loadingMore} onclick={() => void load(false)}>{loadingMore ? "Loading…" : "Load more events"}</button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .c-live-note {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-top: 12px;
    color: var(--mn-fg-2);
    font-size: 14px;
    font-weight: 600;
  }

  .c-live-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--mn-fg-3);
  }

  .c-live-live .c-live-dot {
    background: var(--mn-ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--mn-ok) 15%, transparent);
  }

  .c-live-connecting .c-live-dot {
    background: var(--mn-warn);
  }

  .c-filters {
    margin-top: 24px;
  }

  .c-search {
    display: flex;
    min-width: min(100%, 250px);
    min-height: 44px;
    flex: 1 1 230px;
    align-items: center;
    gap: 9px;
    border: 1px solid var(--mn-line);
    border-radius: 9px;
    background: var(--mn-surface);
    padding: 0 12px;
    color: var(--mn-fg-3);
  }

  .c-search:focus-within {
    outline: 3px solid var(--mn-accent);
    outline-offset: 2px;
  }

  .c-search input {
    width: 100%;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--mn-fg);
    font: inherit;
    font-size: 14px;
  }

  .c-filters select {
    min-width: 140px;
    min-height: 44px;
    border: 1px solid var(--mn-line);
    border-radius: 9px;
    background: var(--mn-surface);
    padding: 0 32px 0 12px;
    color: var(--mn-fg);
    font: inherit;
    font-size: 14px;
  }

  .c-check-filter {
    display: inline-flex;
    min-height: 44px;
    align-items: center;
    gap: 8px;
    border: 1px solid var(--mn-line);
    border-radius: 9px;
    background: var(--mn-surface);
    padding: 0 12px;
    color: var(--mn-fg-2);
    font-size: 14px;
    white-space: nowrap;
  }

  .c-check-filter input {
    width: 16px;
    height: 16px;
    accent-color: var(--mn-accent);
  }

  .c-clear-filter {
    display: inline-flex;
    min-height: 44px;
    align-items: center;
    gap: 5px;
    border: 0;
    background: transparent;
    padding: 0 8px;
    color: var(--mn-accent);
    font-size: 14px;
    font-weight: 600;
  }

  .c-event-count {
    margin: 15px 0 12px;
    color: var(--mn-fg-3);
    font-size: 13px;
  }

  .c-empty-panel {
    border: 1px solid var(--mn-line);
    border-radius: 14px;
    background: var(--mn-surface);
  }

  .c-load-more {
    display: flex;
    justify-content: center;
    margin-top: 18px;
  }

  @media (max-width: 640px) {
    .c-filters select,
    .c-check-filter,
    .c-clear-filter {
      width: 100%;
    }
  }
</style>
