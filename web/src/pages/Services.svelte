<script lang="ts">
  import { ListChecks, Search } from "@lucide/svelte";
  import { api } from "../lib/api";
  import type { ServiceListItem, ServiceState } from "../lib/types";
  import { ago, hostTone, serviceTone } from "../lib/format";
  import { clock, fleet, toastError } from "../lib/state.svelte";
  import { router, serviceHref } from "../lib/router.svelte";
  import Empty from "../lib/components/Empty.svelte";
  import ServiceIcon from "../lib/components/ServiceIcon.svelte";
  import StatusDot from "../lib/components/StatusDot.svelte";

  const q0 = router.route.query;
  let q = $state(q0.get("q") ?? "");
  let stateFilter = $state(q0.get("state") ?? "");
  let services = $state<ServiceListItem[] | null>(null);
  let counts = $state<Partial<Record<ServiceState, number>>>({});
  let gen = 0;

  $effect(() => {
    router.setQuery({ q, state: stateFilter });
  });

  async function load() {
    const my = ++gen;
    try {
      const res = await api.services({ state: stateFilter || null, q: q.trim() || null });
      if (my !== gen) return;
      services = res.services;
      counts = res.counts;
    } catch (e) {
      toastError(e, "Failed to load services");
    }
  }

  let debounce: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void stateFilter;
    void q;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(load, q ? 250 : 0);
  });

  // Reload when a host reports something new (live stream) and on a slow timer.
  const liveSeen = $derived(Object.values(fleet.hosts).reduce((m, h) => Math.max(m, h.last_seen), 0));
  $effect(() => {
    void liveSeen;
    load();
  });
  $effect(() => {
    const t = setInterval(load, 30000);
    return () => clearInterval(t);
  });

  const total = $derived(Object.values(counts).reduce((a, b) => a + (b ?? 0), 0));
  const tabs: [string, string, number][] = $derived([
    ["", "All", total],
    ["failed", "Failed", counts.failed ?? 0],
    ["pending", "Pending", (counts.pending ?? 0) + (counts.init ?? 0)],
    ["unmonitored", "Unmonitored", counts.unmonitored ?? 0],
    ["ok", "Healthy", counts.ok ?? 0],
  ]);
</script>

<div class="space-y-5">
  <div>
    <h1 class="text-2xl font-semibold tracking-tight text-fg">Services</h1>
    <p class="mt-1 text-[13px] text-fg-3">Every check of every host, the ones needing attention first.</p>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <div class="relative min-w-56 flex-1 sm:max-w-xs">
      <Search size={15} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-fg-3" />
      <input class="input pl-9" placeholder="Filter services or hosts…" bind:value={q} aria-label="Filter services" />
    </div>
    <div class="flex flex-wrap rounded-xl border border-line bg-surface-2 p-0.5 text-xs">
      {#each tabs as [id, label, n] (id)}
        <button
          class="flex h-8 items-center gap-1.5 rounded-[9px] px-2.5 font-medium transition-colors {stateFilter === id ? 'bg-surface-solid text-fg shadow-sm ring-1 ring-[var(--line-strong)]' : 'text-fg-3 hover:text-fg-2'}"
          onclick={() => (stateFilter = id)}
        >
          {#if id}<StatusDot tone={serviceTone((id === "pending" ? "pending" : id) as ServiceState)} size={6} />{/if}{label}<span class="num text-[10px] text-fg-3">{n}</span>
        </button>
      {/each}
    </div>
  </div>

  {#if !services}
    <div class="card space-y-3 p-4">{#each Array(8) as _, i (i)}<div class="skeleton h-11 w-full"></div>{/each}</div>
  {:else if services.length === 0}
    <div class="card"><Empty icon={ListChecks} title="No services" body={q || stateFilter ? "Nothing matches these filters." : "No host has reported a check yet."} /></div>
  {:else}
    <div class="card divide-y divide-[var(--line)] overflow-hidden">
      {#each services as s (`${s.host_id}/${s.name}`)}
        <a class="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-[var(--surface-hover)] sm:px-5" href={serviceHref(s.host_id, s.name)}>
          <ServiceIcon type={s.type} size={18} />
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <StatusDot tone={serviceTone(s.state)} size={7} pulse={s.state === "failed"} />
              <span class="truncate font-medium text-fg">{s.name}</span>
            </div>
            <div class="mt-0.5 flex flex-wrap items-center gap-x-2 text-[12px] text-fg-3">
              <span class="inline-flex items-center gap-1.5"><StatusDot tone={hostTone(s.host_state)} size={5} />{s.host}</span>
              <span class={s.state === "failed" ? "text-bad" : ""}>{s.status_text}</span>
              {#if s.pending_action}<span class="text-warn">{s.pending_action} pending</span>{/if}
            </div>
          </div>
          {#if s.state_since}<span class="num shrink-0 text-xs text-fg-3">{ago(s.state_since, clock.now).replace(" ago", "")}</span>{/if}
        </a>
      {/each}
    </div>
  {/if}
</div>
