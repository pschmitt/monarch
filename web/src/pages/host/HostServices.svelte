<script lang="ts">
  import { Ellipsis, Eye, EyeOff, Play, RotateCw, Search, SearchX, Square, X } from "@lucide/svelte";
  import type { HostDetail, Service, ServiceAction, ServiceState, ServiceType } from "../../lib/types";
  import { ago, hostName, serviceTone, serviceTypeLabel, serviceTypeOrder } from "../../lib/format";
  import { fuzzy } from "../../lib/fuzzy";
  import { keyMetrics } from "../../lib/service";
  import { ACTIONS, runAction } from "../../lib/actions";
  import { can, clock } from "../../lib/state.svelte";
  import { router, serviceHref } from "../../lib/router.svelte";
  import Badge from "../../lib/components/Badge.svelte";
  import Empty from "../../lib/components/Empty.svelte";
  import Menu, { type MenuItem } from "../../lib/components/Menu.svelte";
  import ServiceIcon from "../../lib/components/ServiceIcon.svelte";
  import UsageBar from "../../lib/components/UsageBar.svelte";

  let { host, onchange }: { host: HostDetail; onchange: () => void } = $props();

  let q = $state("");
  let stateF = $state<ServiceState | "">((router.route.query.get("state") as ServiceState) ?? "");
  let groupBy = $state<"type" | "group">("type");
  let selected = $state<Set<string>>(new Set());

  const canAct = $derived(can("operator") && host.can_act);
  const icons: Record<ServiceAction, any> = { start: Play, stop: Square, restart: RotateCw, monitor: Eye, unmonitor: EyeOff };

  const filtered = $derived.by(() => {
    let list = host.services;
    if (stateF) list = list.filter((s) => (stateF === "pending" ? s.state === "pending" || s.state === "init" : s.state === stateF));
    if (q.trim()) list = list.filter((s) => fuzzy(q, `${s.name} ${s.status_text} ${s.type}`) >= 0);
    return list;
  });

  const sections = $derived.by(() => {
    const out: { key: string; label: string; type: ServiceType | null; items: Service[] }[] = [];
    const rank = { failed: 0, pending: 1, init: 1, unmonitored: 2, ok: 3 } as const;
    const sortItems = (a: Service, b: Service) => rank[a.state] - rank[b.state] || a.name.localeCompare(b.name, undefined, { numeric: true });
    if (groupBy === "type") {
      for (const t of serviceTypeOrder) {
        const items = filtered.filter((s) => s.type === t).sort(sortItems);
        if (items.length) out.push({ key: t, label: serviceTypeLabel[t], type: t, items });
      }
    } else {
      const groups = new Map<string, Service[]>();
      for (const s of filtered) for (const g of s.groups.length ? s.groups : ["Ungrouped"]) groups.set(g, [...(groups.get(g) ?? []), s]);
      for (const [g, items] of [...groups.entries()].sort((a, b) => (a[0] === "Ungrouped" ? 1 : b[0] === "Ungrouped" ? -1 : a[0].localeCompare(b[0])))) {
        out.push({ key: g, label: g, type: null, items: items.sort(sortItems) });
      }
    }
    return out;
  });

  function toggle(name: string) {
    const s = new Set(selected);
    if (s.has(name)) s.delete(name);
    else s.add(name);
    selected = s;
  }

  async function act(names: string[], a: ServiceAction) {
    if (await runAction(host.id, hostName(host), names, a)) {
      selected = new Set();
      setTimeout(onchange, 1500);
    }
  }

  function menuFor(s: Service): (MenuItem | "sep")[] {
    const items: (MenuItem | "sep")[] = ACTIONS.map((a) => ({
      label: a.label,
      icon: icons[a.id],
      danger: a.danger,
      disabled: (a.id === "monitor" && s.state !== "unmonitored") || (a.id === "unmonitor" && s.state === "unmonitored"),
      onselect: () => act([s.name], a.id),
    }));
    return items;
  }

  const counts = $derived({
    "": host.services.length,
    failed: host.services.filter((s) => s.state === "failed").length,
    pending: host.services.filter((s) => s.state === "pending" || s.state === "init").length,
    unmonitored: host.services.filter((s) => s.state === "unmonitored").length,
    ok: host.services.filter((s) => s.state === "ok").length,
  });
</script>

<div class="space-y-4">
  <div class="flex flex-wrap items-center gap-2">
    <div class="relative min-w-56 flex-1 sm:max-w-xs">
      <Search size={15} class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-fg-3" />
      <input class="input pl-9" placeholder="Filter services…" bind:value={q} aria-label="Filter services" />
    </div>
    <div class="flex flex-wrap rounded-xl border border-line bg-surface-2 p-0.5 text-xs">
      {#each [["", "All"], ["failed", "Failed"], ["pending", "Pending"], ["unmonitored", "Unmonitored"], ["ok", "OK"]] as [id, label] (id)}
        <button
          class="flex h-8 items-center gap-1.5 rounded-[9px] px-2.5 font-medium transition-colors {stateF === id ? 'bg-surface-solid text-fg shadow-sm ring-1 ring-[var(--line-strong)]' : 'text-fg-3 hover:text-fg-2'}"
          onclick={() => (stateF = id as ServiceState | "")}
        >
          {label}<span class="num text-[10px] {id === 'failed' && counts.failed ? 'text-bad' : 'text-fg-3'}">{counts[id as keyof typeof counts]}</span>
        </button>
      {/each}
    </div>
    <div class="ml-auto flex rounded-xl border border-line bg-surface-2 p-0.5 text-xs">
      <button class="h-8 rounded-[9px] px-2.5 font-medium {groupBy === 'type' ? 'bg-surface-solid text-fg ring-1 ring-[var(--line-strong)]' : 'text-fg-3'}" onclick={() => (groupBy = "type")}>By type</button>
      <button class="h-8 rounded-[9px] px-2.5 font-medium {groupBy === 'group' ? 'bg-surface-solid text-fg ring-1 ring-[var(--line-strong)]' : 'text-fg-3'}" onclick={() => (groupBy = "group")}>By group</button>
    </div>
  </div>

  {#if can("operator") && !host.can_act}
    <div class="tone-warn rounded-xl border border-tone-soft bg-tone-soft px-4 py-3 text-[13px] text-fg-2">
      <span class="font-semibold text-tone">Actions unavailable.</span> Monarch doesn't know how to reach Monit's HTTP interface on this host. Enable <code class="num text-xs">set httpd</code> with
      <code class="num text-xs">register credentials</code>, or set a URL override in the host settings.
    </div>
  {/if}

  {#if selected.size > 0}
    <div class="card sticky top-20 z-20 flex flex-wrap items-center gap-2 px-4 py-2.5 animate-in">
      <span class="text-[13px] font-medium text-fg">{selected.size} selected</span>
      <div class="mx-1 h-5 w-px bg-[var(--line)]"></div>
      {#each ACTIONS as a (a.id)}
        {@const Icon = icons[a.id]}
        <button class="btn btn-sm {a.danger ? 'text-bad' : ''}" onclick={() => act([...selected], a.id)}><Icon size={13} />{a.label}</button>
      {/each}
      <button class="btn btn-ghost btn-sm btn-icon ml-auto" onclick={() => (selected = new Set())} aria-label="Clear selection"><X size={14} /></button>
    </div>
  {/if}

  {#if sections.length === 0}
    <div class="card"><Empty icon={SearchX} title="No services match" body="Try clearing the filters." /></div>
  {/if}

  {#each sections as sec (sec.key)}
    <section class="card overflow-hidden">
      <div class="flex items-center gap-2.5 border-b border-line px-4 py-3 sm:px-5">
        {#if sec.type}<span class="text-fg-3"><ServiceIcon type={sec.type} size={15} /></span>{/if}
        <h3 class="card-title">{sec.label}</h3>
        <span class="num text-[11px] text-fg-3">{sec.items.length}</span>
        {#if sec.items.some((s) => s.state === "failed")}
          <Badge tone="bad" size="sm" dot>{sec.items.filter((s) => s.state === "failed").length} failed</Badge>
        {/if}
      </div>
      <ul class="divide-y divide-[var(--line)]">
        {#each sec.items as s (s.id)}
          {@const tone = serviceTone(s.state)}
          {@const metrics = keyMetrics(s)}
          <li class="group flex items-center gap-3 px-4 py-2.5 transition-colors hover:bg-hover sm:px-5 {selected.has(s.name) ? 'bg-hover' : ''}">
            {#if canAct}
              <input type="checkbox" class="h-4 w-4 shrink-0 accent-[var(--accent)]" checked={selected.has(s.name)} onchange={() => toggle(s.name)} aria-label="Select {s.name}" />
            {/if}
            <a href={serviceHref(host.id, s.name)} class="flex min-w-0 flex-1 items-center gap-3">
              <span class="tone-{tone} flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-tone-soft bg-tone-soft text-tone">
                <ServiceIcon type={s.type} size={15} />
              </span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="truncate text-[13px] font-medium text-fg group-hover:underline">{s.name}</span>
                  {#if s.every}<span class="hidden rounded border border-line px-1 text-[10px] text-fg-3 sm:inline">{s.every}</span>{/if}
                </div>
                <div class="flex items-center gap-2 text-[11px]">
                  <span class="tone-{tone} text-tone">{s.status_text}</span>
                  {#if s.state !== "ok" && s.state_since}<span class="num text-fg-3">· {ago(s.state_since, clock.now)}</span>{/if}
                  {#if s.pending_action}<span class="text-warn">· {s.pending_action} pending</span>{/if}
                </div>
              </div>
              <div class="hidden items-center gap-5 md:flex">
                {#each metrics as m (m.label)}
                  <div class="w-24 text-right">
                    <div class="text-[10px] tracking-wide text-fg-3 uppercase">{m.label}</div>
                    <div class="num truncate text-xs text-fg">{m.value}</div>
                    {#if m.percent !== undefined && m.percent !== null}<div class="mt-1"><UsageBar value={m.percent} height={3} /></div>{/if}
                  </div>
                {/each}
              </div>
            </a>
            {#if canAct}
              <Menu items={menuFor(s)} label="Actions for {s.name}">
                {#snippet trigger()}<Ellipsis size={16} />{/snippet}
              </Menu>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>
