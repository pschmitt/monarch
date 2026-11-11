<script lang="ts">
  import { ArrowRightLeft, Check, Circle, Clock3, ExternalLink, HeartPulse, X } from "@lucide/svelte";
  import { api } from "../../api";
  import type { EventState, MonarchEvent } from "../../types";
  import { ago, datetime, eventStateLabel } from "../../format";
  import { can, toastError } from "../../state.svelte";
  import { hostHref, serviceHref } from "../../router.svelte";
  import Modal from "../Modal.svelte";

  let { events, compact = false }: { events: MonarchEvent[]; compact?: boolean } = $props();
  let selectedId = $state<number | null>(null);
  let modalOpen = $state(false);
  let acking = $state(false);
  let acknowledgements = $state<Record<number, MonarchEvent>>({});

  const selected = $derived(selectedId === null ? null : acknowledgements[selectedId] ?? events.find((event) => event.id === selectedId) ?? null);

  function tone(state: EventState): string {
    if (state === "succeeded") return "ok";
    if (state === "failed") return "failed";
    if (state === "changed") return "change";
    if (state === "init") return "pending";
    return "stale";
  }

  function grouped() {
    const groups: { day: string; events: MonarchEvent[] }[] = [];
    for (const event of events) {
      const date = new Date(event.created_at * 1000);
      const now = new Date();
      const yesterday = new Date(Date.now() - 86400000);
      const day = date.toDateString() === now.toDateString()
        ? "Today"
        : date.toDateString() === yesterday.toDateString()
          ? "Yesterday"
          : date.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric", year: date.getFullYear() === now.getFullYear() ? undefined : "numeric" });
      if (groups.at(-1)?.day !== day) groups.push({ day, events: [] });
      groups.at(-1)!.events.push(event);
    }
    return groups;
  }

  const groups = $derived(grouped());

  async function acknowledge() {
    if (!selected || !can("operator") || acking) return;
    acking = true;
    try {
      const updated = await api.ackEvent(selected.id);
      acknowledgements = { ...acknowledgements, [selected.id]: updated };
    } catch (error) {
      toastError(error, "Could not acknowledge");
    } finally {
      acking = false;
    }
  }
</script>

<div class="c-timeline" class:c-timeline-compact={compact}>
  {#each groups as group (group.events[0]?.id)}
    <h2 class="c-section-title c-timeline-day">{group.day}</h2>
    <div class="c-timeline-list">
      {#each group.events as event (event.id)}
        {@const shown = acknowledgements[event.id] ?? event}
        {@const stateTone = tone(shown.state)}
        {@const Icon = shown.kind === "heartbeat" ? HeartPulse : shown.state === "succeeded" ? Check : shown.state === "failed" ? X : shown.state === "init" ? Clock3 : shown.state === "changed" ? ArrowRightLeft : Circle}
        <button
          type="button"
          class="c-timeline-row"
          onclick={() => { selectedId = shown.id; modalOpen = true; }}
          aria-label="View event details: {shown.message}"
        >
          <time class="c-timeline-time" datetime={new Date(shown.created_at * 1000).toISOString()}>{new Date(shown.created_at * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}</time>
          <span class="c-timeline-marker c-event-{stateTone}" aria-hidden="true"><Icon size={17} strokeWidth={2.3} /></span>
          <span class="c-timeline-copy">
            <span class="c-timeline-heading">
              {#if shown.host}<strong>{shown.host}</strong>{/if}
              {#if shown.host && shown.service}<span aria-hidden="true">/</span>{/if}
              {#if shown.service}<strong>{shown.service}</strong>{/if}
              {#if !shown.host && !shown.service}<strong>{shown.kind_label}</strong>{/if}
              <span class="c-event-state">{eventStateLabel[shown.state]}</span>
            </span>
            <span class="c-timeline-message">{shown.message}</span>
            {#if !compact && shown.acked_by}<span class="c-timeline-ack"><Check size={13} /> Acknowledged by {shown.acked_by}</span>{/if}
          </span>
          <span class="c-timeline-open" aria-hidden="true"><ExternalLink size={15} /></span>
        </button>
      {/each}
    </div>
  {/each}
</div>

<Modal bind:open={modalOpen} title={selected?.message ?? "Event details"} width="max-w-xl" onclose={() => { selectedId = null; }}>
  {#if selected}
    {@const stateTone = tone(selected.state)}
    <div class="c-event-detail">
      <div class="c-event-detail-state">
        <span class="c-event-detail-icon c-event-{stateTone}">
          {#if selected.state === "succeeded"}<Check size={17} />
          {:else if selected.state === "failed"}<X size={17} />
          {:else if selected.state === "init"}<Clock3 size={17} />
          {:else if selected.state === "changed"}<ArrowRightLeft size={17} />
          {:else}<Circle size={17} />{/if}
        </span>
        <span>{eventStateLabel[selected.state]}</span>
      </div>
      <dl class="c-event-facts">
        <dt>When</dt><dd>{datetime(selected.created_at)}</dd>
        <dt>Event</dt><dd>{selected.kind_label || selected.kind}</dd>
        <dt>Host</dt><dd>{#if selected.host_id !== null && selected.host}<a href={hostHref(selected.host_id)}>{selected.host}</a>{:else}{selected.host ?? "—"}{/if}</dd>
        <dt>Service</dt><dd>{#if selected.host_id !== null && selected.service}<a href={serviceHref(selected.host_id, selected.service)}>{selected.service}</a>{:else}{selected.service ?? "—"}{/if}</dd>
        <dt>Source</dt><dd>{selected.source}</dd>
        {#if selected.action}<dt>Action</dt><dd>{selected.action}</dd>{/if}
        {#if selected.acked_by}<dt>Acknowledged</dt><dd>{selected.acked_by}{selected.acked_at ? ` · ${datetime(selected.acked_at)}` : ""}</dd>{/if}
      </dl>
      <p class="c-event-full-message">{selected.message}</p>
      {#if can("operator") && !selected.acked_by}
        <button class="btn btn-primary mt-5" type="button" disabled={acking} onclick={acknowledge}>
          <Check size={15} /> {acking ? "Acknowledging…" : "Acknowledge event"}
        </button>
      {/if}
      <p class="c-event-relative" title={datetime(selected.created_at)}>{ago(selected.created_at)}</p>
    </div>
  {/if}
</Modal>

<style>
  .c-timeline-day {
    margin: 28px 0 8px;
  }

  .c-timeline-day:first-child {
    margin-top: 2px;
  }

  .c-timeline-list {
    position: relative;
    overflow: hidden;
    border: 1px solid var(--mn-line);
    border-radius: 14px;
    background: var(--mn-surface);
    box-shadow: var(--mn-shadow);
  }

  .c-timeline-row {
    position: relative;
    display: grid;
    width: 100%;
    min-height: 74px;
    grid-template-columns: 68px 30px minmax(0, 1fr) 18px;
    align-items: center;
    gap: 14px;
    border: 0;
    border-bottom: 1px solid var(--mn-line);
    background: transparent;
    padding: 14px 18px;
    color: var(--mn-fg);
    text-align: left;
    transition: background-color 120ms ease;
  }

  .c-timeline-row:last-child {
    border-bottom: 0;
  }

  .c-timeline-row:hover,
  .c-timeline-row:focus-visible {
    background: var(--mn-surface-hover);
  }

  .c-timeline-time {
    color: var(--mn-fg-3);
    font-size: 14px;
    font-variant-numeric: tabular-nums;
    text-align: right;
    white-space: nowrap;
  }

  .c-timeline-marker {
    position: relative;
    z-index: 1;
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border-radius: 50%;
    background: var(--mn-surface);
  }

  .c-timeline-marker::before {
    position: absolute;
    z-index: -1;
    top: -25px;
    bottom: -25px;
    left: 50%;
    width: 1px;
    background: var(--mn-line);
    content: "";
  }

  .c-timeline-copy {
    display: grid;
    min-width: 0;
    gap: 3px;
  }

  .c-timeline-heading {
    display: flex;
    min-width: 0;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px 8px;
    font-size: 15px;
  }

  .c-timeline-heading strong {
    overflow-wrap: anywhere;
    font-weight: 650;
  }

  .c-event-state {
    border-radius: 14px;
    background: var(--mn-surface-2);
    padding: 2px 8px;
    color: var(--mn-fg-2);
    font-size: 12px;
    font-weight: 650;
    white-space: nowrap;
  }

  .c-timeline-message {
    overflow: hidden;
    color: var(--mn-fg-2);
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .c-timeline-ack {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--mn-fg-3);
    font-size: 12px;
  }

  .c-timeline-open {
    color: var(--mn-fg-3);
    opacity: 0;
    transition: opacity 120ms ease;
  }

  .c-timeline-row:hover .c-timeline-open,
  .c-timeline-row:focus-visible .c-timeline-open {
    opacity: 1;
  }

  .c-timeline-compact .c-timeline-day {
    margin-top: 20px;
    font-size: 18px;
  }

  .c-timeline-compact .c-timeline-row {
    min-height: 64px;
    padding-block: 10px;
  }

  .c-event-detail-state {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 20px;
    font-weight: 650;
  }

  .c-event-detail-icon {
    flex: 0 0 30px;
  }

  .c-event-facts {
    display: grid;
    grid-template-columns: minmax(90px, 130px) minmax(0, 1fr);
    gap: 10px 18px;
    margin: 0;
    font-size: 14px;
  }

  .c-event-facts dt {
    color: var(--mn-fg-3);
  }

  .c-event-facts dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .c-event-facts a {
    color: var(--mn-accent);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .c-event-full-message {
    margin-top: 22px;
    border-top: 1px solid var(--mn-line);
    padding-top: 17px;
    overflow-wrap: anywhere;
    color: var(--mn-fg-2);
    font-size: 14px;
  }

  .c-event-relative {
    margin-top: 14px;
    color: var(--mn-fg-3);
    font-size: 12px;
  }

  @media (max-width: 640px) {
    .c-timeline-row {
      grid-template-columns: 46px 26px minmax(0, 1fr);
      gap: 8px;
      padding: 12px 10px;
    }

    .c-timeline-time {
      font-size: 12px;
    }

    .c-timeline-marker {
      width: 26px;
      height: 26px;
    }

    .c-timeline-open {
      display: none;
    }

    .c-timeline-message {
      white-space: normal;
    }

    .c-event-facts {
      grid-template-columns: 80px minmax(0, 1fr);
      gap: 8px 12px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .c-timeline-row,
    .c-timeline-open {
      transition: none;
    }
  }
</style>
