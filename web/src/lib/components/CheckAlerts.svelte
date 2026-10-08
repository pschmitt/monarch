<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import { api } from "../api";
  import type { Channel, EventKind } from "../types";
  import { toast, toastError } from "../state.svelte";
  import Modal from "./Modal.svelte";
  import Switch from "./Switch.svelte";

  let {
    open = $bindable(false),
    hostId,
    service,
    hostName = "",
    onsaved,
  }: { open?: boolean; hostId: number; service: string; hostName?: string; onsaved?: () => void } = $props();

  let kinds = $state<EventKind[]>([]);
  let channels = $state<Channel[]>([]);
  let muted = $state(false);
  let ownEvents = $state(false);
  let events = $state<string[]>([]);
  let ownChannels = $state(false);
  let selected = $state<number[]>([]);
  let loading = $state(false);
  let busy = $state(false);

  $effect(() => {
    if (!open) return;
    loading = true;
    Promise.all([api.checkAlert(hostId, service), api.eventKinds(), api.channels()])
      .then(([a, k, c]) => {
        kinds = k;
        channels = c;
        muted = a.muted;
        ownEvents = a.events !== null;
        events = a.events ?? [];
        ownChannels = a.channels !== null;
        selected = a.channels ?? [];
      })
      .catch((e) => toastError(e, "Failed to load alert settings"))
      .finally(() => (loading = false));
  });

  const toggle = <T,>(list: T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);

  async function save() {
    busy = true;
    try {
      await api.saveCheckAlert(hostId, service, { muted, events: ownEvents ? events : null, channels: ownChannels ? selected : null });
      toast("ok", "Alert settings saved");
      open = false;
      onsaved?.();
    } catch (e) {
      toastError(e, "Could not save alert settings");
    } finally {
      busy = false;
    }
  }

  async function reset() {
    busy = true;
    try {
      await api.resetCheckAlert(hostId, service);
      toast("ok", "Back to the generic settings");
      open = false;
      onsaved?.();
    } catch (e) {
      toastError(e, "Could not reset");
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="Alerts for {service}{hostName ? ` on ${hostName}` : ''}" width="max-w-xl">
  {#if loading}
    <div class="skeleton h-40 w-full"></div>
  {:else}
    <div class="space-y-5">
      <p class="text-xs text-fg-3">Settings here win over the generic event routing, for this check only.</p>

      <label class="flex items-center justify-between gap-4 text-[13px] text-fg-2">
        <span>
          Mute this check
          <span class="block text-[11px] text-fg-3">No notifications at all. Events are still recorded.</span>
        </span>
        <Switch bind:checked={muted} label="Mute this check" />
      </label>

      <fieldset class="space-y-2 rounded-xl border border-line p-4 {muted ? 'opacity-50' : ''}" disabled={muted}>
        <legend class="label px-1">Events</legend>
        <label class="flex items-center gap-2 text-[13px] text-fg-2"><input type="radio" name="ev" checked={!ownEvents} onchange={() => (ownEvents = false)} class="accent-[var(--accent)]" /> Follow the generic event settings</label>
        <label class="flex items-center gap-2 text-[13px] text-fg-2"><input type="radio" name="ev" checked={ownEvents} onchange={() => (ownEvents = true)} class="accent-[var(--accent)]" /> Only these kinds</label>
        {#if ownEvents}
          <div class="flex flex-wrap gap-1.5 pt-1">
            {#each kinds as k (k.kind)}
              <button
                type="button"
                class="rounded-md border px-2 py-1 text-[11px] transition-colors {events.includes(k.kind) ? 'border-[color-mix(in_oklab,var(--accent)_60%,transparent)] bg-[color-mix(in_oklab,var(--accent)_10%,transparent)] text-fg' : 'border-line text-fg-3 hover:text-fg-2'}"
                aria-pressed={events.includes(k.kind)}
                title="{k.failed} / {k.succeeded}"
                onclick={() => (events = toggle(events, k.kind))}>{k.failed.replace(/ failed$| exceeded$/, "")}</button
              >
            {/each}
          </div>
        {/if}
      </fieldset>

      <fieldset class="space-y-2 rounded-xl border border-line p-4 {muted ? 'opacity-50' : ''}" disabled={muted}>
        <legend class="label px-1">Channels</legend>
        <label class="flex items-center gap-2 text-[13px] text-fg-2"><input type="radio" name="ch" checked={!ownChannels} onchange={() => (ownChannels = false)} class="accent-[var(--accent)]" /> Normal routing</label>
        <label class="flex items-center gap-2 text-[13px] text-fg-2"><input type="radio" name="ch" checked={ownChannels} onchange={() => (ownChannels = true)} class="accent-[var(--accent)]" /> Only these channels</label>
        {#if ownChannels}
          <div class="grid gap-1.5 pt-1 sm:grid-cols-2">
            {#each channels as c (c.id)}
              <label class="flex items-center gap-2 text-[13px] text-fg-2">
                <input type="checkbox" checked={selected.includes(c.id)} onchange={() => (selected = toggle(selected, c.id))} class="accent-[var(--accent)]" />
                <span class="truncate">{c.name}</span>
                {#if !c.enabled}<span class="text-[10px] text-fg-3">(off)</span>{/if}
              </label>
            {:else}
              <span class="text-xs text-fg-3">No channels yet.</span>
            {/each}
          </div>
        {/if}
      </fieldset>
    </div>
  {/if}
  {#snippet footer()}
    <button class="btn btn-ghost mr-auto" onclick={reset} disabled={busy || loading}>Reset to generic</button>
    <button class="btn btn-ghost" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" onclick={save} disabled={busy || loading}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Save</button>
  {/snippet}
</Modal>
