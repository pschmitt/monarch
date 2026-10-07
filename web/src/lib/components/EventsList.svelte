<script lang="ts">
  import { CheckCheck, Inbox, LoaderCircle, Search } from "@lucide/svelte";
  import { api } from "../api";
  import type { EventState, MonarchEvent } from "../types";
  import { eventStateLabel, eventTone, hostName } from "../format";
  import { can, hostList, onLiveEvent, toast, toastError } from "../state.svelte";
  import Empty from "./Empty.svelte";
  import EventRow from "./EventRow.svelte";
  import StatusDot from "./StatusDot.svelte";

  let {
    host = null,
    service = null,
    filters = true,
    pageSize = 50,
  }: { host?: number | null; service?: string | null; filters?: boolean; pageSize?: number } = $props();

  let events = $state<MonarchEvent[]>([]);
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let q = $state("");
  let stateF = $state<EventState | "">("");
  let hostF = $state<number | "">("");
  let unacked = $state(false);
  let selected = $state<Set<number>>(new Set());
  let sentinel = $state<HTMLDivElement>();
  let gen = 0;

  const effHost = $derived(host ?? (hostF === "" ? null : hostF));

  async function load(reset: boolean) {
    const my = reset ? ++gen : gen;
    if (reset) loading = true;
    else loadingMore = true;
    try {
      const res = await api.events({
        host: effHost,
        service,
        state: stateF || null,
        q: q.trim() || null,
        unacked,
        limit: pageSize,
        before: reset ? null : (events.at(-1)?.id ?? null),
      });
      if (my !== gen) return;
      events = reset ? res.events : [...events, ...res.events.filter((e) => !events.some((x) => x.id === e.id))];
      hasMore = res.has_more;
      if (reset) selected = new Set();
    } catch (e) {
      toastError(e, "Failed to load events");
    } finally {
      if (my === gen) {
        loading = false;
        loadingMore = false;
      }
    }
  }

  let debounce: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    void effHost;
    void service;
    void stateF;
    void unacked;
    void q;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => load(true), q ? 250 : 0);
  });

  // live prepend
  $effect(() =>
    onLiveEvent((e) => {
      if (effHost !== null && e.host_id !== effHost) return;
      if (service && e.service !== service) return;
      if (stateF && e.state !== stateF) return;
      if (q.trim() && !`${e.host} ${e.service} ${e.message}`.toLowerCase().includes(q.trim().toLowerCase())) return;
      if (!events.some((x) => x.id === e.id)) events = [e, ...events];
    }),
  );

  $effect(() => {
    if (!sentinel) return;
    const io = new IntersectionObserver((entries) => {
      if (entries[0]?.isIntersecting && hasMore && !loading && !loadingMore) load(false);
    }, { rootMargin: "400px" });
    io.observe(sentinel);
    return () => io.disconnect();
  });

  async function ack(e: MonarchEvent) {
    try {
      const updated = await api.ackEvent(e.id);
      events = events.map((x) => (x.id === e.id ? updated : x));
    } catch (err) {
      toastError(err, "Could not acknowledge");
    }
  }

  async function ackSelected() {
    const ids = [...selected];
    if (!ids.length) return;
    try {
      await api.ackEvents(ids);
      const me = "you";
      events = events.map((x) => (selected.has(x.id) ? { ...x, acked_by: x.acked_by ?? me, acked_at: Date.now() / 1000 } : x));
      toast("ok", `Acknowledged ${ids.length} event${ids.length === 1 ? "" : "s"}`);
      selected = new Set();
    } catch (err) {
      toastError(err, "Could not acknowledge");
    }
  }

  function toggle(id: number) {
    const s = new Set(selected);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selected = s;
  }

  const unackedVisible = $derived(events.filter((e) => !e.acked_by));
  const allSelected = $derived(unackedVisible.length > 0 && unackedVisible.every((e) => selected.has(e.id)));
  const canAck = $derived(can("operator"));
  const hosts = $derived(hostList());

  // group by day
  const grouped = $derived.by(() => {
    const out: { day: string; items: MonarchEvent[] }[] = [];
    for (const e of events) {
      const d = new Date(e.created_at * 1000);
      const today = new Date();
      const yest = new Date(Date.now() - 86400000);
      const label =
        d.toDateString() === today.toDateString()
          ? "Today"
          : d.toDateString() === yest.toDateString()
            ? "Yesterday"
            : d.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
      if (out.at(-1)?.day !== label) out.push({ day: label, items: [] });
      out.at(-1)!.items.push(e);
    }
    return out;
  });
</script>

<div class="space-y-4">
  {#if filters}
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative min-w-56 flex-1 sm:max-w-sm">
        <Search size={15} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-fg-3" />
        <input class="input pl-9" placeholder="Search messages, services…" bind:value={q} aria-label="Search events" />
      </div>
      {#if host === null}
        <select class="input h-9 w-auto min-w-36 text-xs" bind:value={hostF} aria-label="Host">
          <option value="">All hosts</option>
          {#each hosts as h (h.id)}<option value={h.id}>{hostName(h)}</option>{/each}
        </select>
      {/if}
      <div class="flex flex-wrap rounded-xl border border-line bg-surface-2 p-0.5 text-xs">
        {#each ["", "failed", "succeeded", "changed"] as s (s)}
          <button
            class="flex h-8 items-center gap-1.5 rounded-[9px] px-2.5 font-medium transition-colors {stateF === s ? 'bg-surface-solid text-fg shadow-sm ring-1 ring-[var(--line-strong)]' : 'text-fg-3 hover:text-fg-2'}"
            onclick={() => (stateF = s as EventState | "")}
          >
            {#if s}<StatusDot tone={eventTone(s as EventState)} size={6} />{eventStateLabel[s as EventState]}{:else}All{/if}
          </button>
        {/each}
      </div>
      <label class="flex h-9 cursor-pointer items-center gap-2 rounded-xl border border-line bg-surface-2 px-3 text-xs text-fg-2">
        <input type="checkbox" class="accent-[var(--accent)]" bind:checked={unacked} /> Unacknowledged
      </label>
    </div>
  {/if}

  {#if canAck && unackedVisible.length > 0}
    <div class="flex items-center gap-3 text-xs text-fg-3">
      <label class="flex cursor-pointer items-center gap-2">
        <input
          type="checkbox"
          class="h-4 w-4 accent-[var(--accent)]"
          checked={allSelected}
          onchange={() => (selected = allSelected ? new Set() : new Set(unackedVisible.map((e) => e.id)))}
        />
        Select all unacknowledged
      </label>
      {#if selected.size}
        <button class="btn btn-sm btn-primary animate-in" onclick={ackSelected}><CheckCheck size={14} /> Acknowledge {selected.size}</button>
      {/if}
    </div>
  {/if}

  <div class="card overflow-hidden">
    {#if loading}
      <div class="space-y-4 p-5">{#each Array(8) as _, i (i)}<div class="flex gap-3"><div class="skeleton h-7 w-7"></div><div class="flex-1 space-y-2"><div class="skeleton h-3.5 w-1/3"></div><div class="skeleton h-3 w-2/3"></div></div></div>{/each}</div>
    {:else if events.length === 0}
      <Empty icon={Inbox} title="No events" body={q || stateF || unacked ? "Nothing matches these filters." : "Monit hasn't reported any events yet."} />
    {:else}
      {#each grouped as g (g.day)}
        <div class="border-b border-line bg-[var(--surface-hover)] px-5 py-2 eyebrow">{g.day}</div>
        <div class="divide-y divide-[var(--line)]">
          {#each g.items as e (e.id)}
            <EventRow
              event={e}
              showHost={host === null}
              selectable={canAck && !e.acked_by}
              selected={selected.has(e.id)}
              ontoggle={() => toggle(e.id)}
              onack={canAck ? () => ack(e) : undefined}
            />
          {/each}
        </div>
      {/each}
      <div bind:this={sentinel} class="flex h-14 items-center justify-center text-xs text-fg-3">
        {#if loadingMore}<LoaderCircle size={16} class="animate-spin" />{:else if hasMore}<button class="btn btn-sm" onclick={() => load(false)}>Load more</button>{:else}<span>That's all — {events.length} event{events.length === 1 ? "" : "s"}</span>{/if}
      </div>
    {/if}
  </div>
</div>
