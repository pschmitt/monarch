<script lang="ts">
  import { api } from "../api";
  import type { EventKind } from "../types";
  import { toast, toastError } from "../state.svelte";
  import Switch from "./Switch.svelte";

  let kinds = $state<EventKind[] | null>(null);
  let disabled = $state<string[]>([]);

  async function load() {
    try {
      [kinds, disabled] = await Promise.all([api.eventKinds(), api.settings().then((s) => s.disabled_events ?? [])]);
    } catch (e) {
      toastError(e, "Failed to load event types");
    }
  }
  $effect(() => {
    load();
  });

  async function toggle(kind: string, on: boolean) {
    const next = on ? disabled.filter((k) => k !== kind) : [...disabled, kind];
    const prev = disabled;
    disabled = next;
    try {
      const s = await api.updateSettings({ disabled_events: next });
      disabled = s.disabled_events ?? next;
    } catch (e) {
      disabled = prev;
      toastError(e, "Could not update event types");
    }
  }

  async function all(on: boolean) {
    if (!kinds) return;
    const next = on ? [] : kinds.map((k) => k.kind);
    try {
      const s = await api.updateSettings({ disabled_events: next });
      disabled = s.disabled_events ?? next;
      toast("ok", on ? "All event types enabled" : "All event types muted");
    } catch (e) {
      toastError(e, "Could not update event types");
    }
  }
</script>

<section class="card p-5">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <h2 class="card-title">Event types</h2>
      <p class="mt-0.5 text-xs text-fg-3">Turn kinds of events on or off for all channels. Muted kinds are still recorded, they just never notify.</p>
    </div>
    <div class="flex gap-1.5">
      <button class="btn btn-ghost btn-sm" onclick={() => all(true)}>Enable all</button>
      <button class="btn btn-ghost btn-sm" onclick={() => all(false)}>Mute all</button>
    </div>
  </div>
  {#if !kinds}
    <div class="skeleton mt-4 h-24 w-full"></div>
  {:else}
    <div class="mt-4 grid gap-x-6 gap-y-2.5 sm:grid-cols-2 lg:grid-cols-3">
      {#each kinds as k (k.kind)}
        <label class="flex items-center justify-between gap-3 text-[13px] text-fg-2">
          <span class="min-w-0 truncate" title="{k.failed} / {k.succeeded}">{k.failed.replace(/ failed$| exceeded$/, "")}</span>
          <Switch checked={!disabled.includes(k.kind)} label={k.failed} onchange={(v) => toggle(k.kind, v)} />
        </label>
      {/each}
    </div>
  {/if}
</section>
