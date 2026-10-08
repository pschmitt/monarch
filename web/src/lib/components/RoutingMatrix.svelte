<script lang="ts">
  import { api } from "../api";
  import type { Channel, EventKind } from "../types";
  import { toast, toastError } from "../state.svelte";
  import Switch from "./Switch.svelte";

  let { channels, onchange }: { channels: Channel[]; onchange: () => void } = $props();

  let kinds = $state<EventKind[] | null>(null);
  let disabled = $state<string[]>([]);
  let groupMinutes = $state(10);
  let savedGroup = 10;

  async function load() {
    try {
      const [k, s] = await Promise.all([api.eventKinds(), api.settings()]);
      kinds = k;
      disabled = s.disabled_events ?? [];
      groupMinutes = savedGroup = s.group_minutes ?? 10;
    } catch (e) {
      toastError(e, "Failed to load event types");
    }
  }
  $effect(() => {
    load();
  });

  const routed = $derived(channels.filter((c) => !c.default));
  const fallback = $derived(channels.find((c) => c.default));
  const accepts = (c: Channel, kind: string) => c.filter.events === null || c.filter.events.includes(kind);

  async function setMuted(kind: string, notify: boolean) {
    const next = notify ? disabled.filter((k) => k !== kind) : [...disabled, kind];
    const prev = disabled;
    disabled = next;
    try {
      disabled = (await api.updateSettings({ disabled_events: next })).disabled_events ?? next;
    } catch (e) {
      disabled = prev;
      toastError(e, "Could not update event types");
    }
  }

  async function setAll(notify: boolean) {
    if (!kinds) return;
    const next = notify ? [] : kinds.map((k) => k.kind);
    try {
      disabled = (await api.updateSettings({ disabled_events: next })).disabled_events ?? next;
      toast("ok", notify ? "All event types enabled" : "All event types muted");
    } catch (e) {
      toastError(e, "Could not update event types");
    }
  }

  async function route(c: Channel, kind: string, on: boolean) {
    if (!kinds) return;
    const all = kinds.map((k) => k.kind);
    const base = c.filter.events ?? all;
    const next = on ? [...new Set([...base, kind])] : base.filter((k) => k !== kind);
    try {
      // Every kind selected is stored as "all", so newly added kinds are included too.
      await api.updateChannel(c.id, { filter: { ...c.filter, events: next.length === all.length ? null : next } });
      onchange();
    } catch (e) {
      toastError(e, "Could not update routing");
    }
  }

  async function saveGroup() {
    const m = Math.max(0, Math.min(1440, Math.round(Number(groupMinutes) || 0)));
    groupMinutes = m;
    if (m === savedGroup) return;
    try {
      savedGroup = (await api.updateSettings({ group_minutes: m })).group_minutes ?? m;
      toast("ok", m === 0 ? "Notifications are sent immediately" : `Notifications are grouped for ${m} min`);
    } catch (e) {
      toastError(e, "Could not save the grouping window");
    }
  }
</script>

<section class="card p-5">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="max-w-xl">
      <h2 class="card-title">Event routing</h2>
      <p class="mt-0.5 text-xs text-fg-3">
        Choose which channels get which events. Events no channel claims go to the default channel{fallback ? ` (${fallback.name})` : ", if you set one"}. Muted events are still recorded, they just never notify.
      </p>
    </div>
    <div class="flex flex-wrap items-end gap-3">
      <label class="text-xs text-fg-2">
        <span class="label">Group notifications for</span>
        <span class="flex items-center gap-2">
          <input class="input num h-8 w-20 text-center text-xs" type="number" min="0" max="1440" bind:value={groupMinutes} onchange={saveGroup} aria-label="Grouping window in minutes" />
          <span class="text-fg-3">min (0 = off)</span>
        </span>
      </label>
      <div class="flex gap-1.5">
        <button class="btn btn-ghost btn-sm" onclick={() => setAll(true)}>Enable all</button>
        <button class="btn btn-ghost btn-sm" onclick={() => setAll(false)}>Mute all</button>
      </div>
    </div>
  </div>
  {#if !kinds}
    <div class="skeleton mt-4 h-24 w-full"></div>
  {:else}
    <div class="mt-4 overflow-x-auto">
      <table class="table min-w-[520px]">
        <thead>
          <tr>
            <th>Event</th>
            <th class="text-center">Notify</th>
            {#each routed as c (c.id)}<th class="text-center" title={c.name}><span class="block max-w-24 truncate">{c.name}</span></th>{/each}
            <th class="text-center">{fallback ? "Default" : ""}</th>
          </tr>
        </thead>
        <tbody>
          {#each kinds as k (k.kind)}
            {@const muted = disabled.includes(k.kind)}
            {@const claimed = routed.some((c) => c.enabled && accepts(c, k.kind))}
            <tr class={muted ? "opacity-50" : ""}>
              <td class="text-[13px] text-fg-2" title="{k.failed} / {k.succeeded}">{k.failed.replace(/ failed$| exceeded$/, "")}</td>
              <td class="text-center"><Switch checked={!muted} label="Notify about {k.failed}" onchange={(v) => setMuted(k.kind, v)} /></td>
              {#each routed as c (c.id)}
                <td class="text-center">
                  <input
                    type="checkbox"
                    class="h-4 w-4 accent-[var(--accent)]"
                    checked={accepts(c, k.kind)}
                    disabled={muted}
                    aria-label="{k.failed} to {c.name}"
                    onchange={(e) => route(c, k.kind, e.currentTarget.checked)}
                  />
                </td>
              {/each}
              <td class="text-center text-[11px] text-fg-3">{#if fallback && !claimed && !muted}<span class="rounded-md border border-line px-1.5 py-0.5">fallback</span>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>
