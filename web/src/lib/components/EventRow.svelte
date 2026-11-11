<script lang="ts">
  import { ArrowRightLeft, CircleCheck, CircleX, HeartPulse, Check, Circle, ArrowUpRight } from "@lucide/svelte";
  import type { MonarchEvent } from "../types";
  import { ago, datetime, timeShort, eventStateLabel, eventTone } from "../format";
  import { clock } from "../state.svelte";
  import { hostHref, serviceHref } from "../router.svelte";
  import Modal from "./Modal.svelte";

  let {
    event,
    compact = false,
    timeline = false,
    showHost = true,
    selectable = false,
    selected = false,
    ontoggle,
    onack,
  }: {
    event: MonarchEvent;
    compact?: boolean;
    timeline?: boolean;
    showHost?: boolean;
    selectable?: boolean;
    selected?: boolean;
    ontoggle?: () => void;
    onack?: () => void;
  } = $props();

  const tone = $derived(eventTone(event.state));
  const Icon = $derived(
    event.kind === "heartbeat" ? HeartPulse : event.state === "failed" ? CircleX : event.state === "succeeded" ? CircleCheck : event.state === "changed" ? ArrowRightLeft : Circle,
  );
  const message = $derived(event.message || event.kind_label || "State changed");
  let detailsOpen = $state(false);
</script>

{#if timeline}
  <article class="timeline-row tone-{tone} {event.acked_by ? 'timeline-acked' : ''}">
    <div class="timeline-gutter">
      {#if selectable}
        <input type="checkbox" class="timeline-select" checked={selected} onchange={() => ontoggle?.()} aria-label="Select event from {event.host ?? 'unknown host'}" />
      {/if}
      <time class="timeline-time num" title={datetime(event.created_at)} datetime={new Date(event.created_at * 1000).toISOString()}>
        {timeShort(event.created_at)}
        <span>{ago(event.created_at, clock.now)}</span>
      </time>
      <span class="timeline-marker" aria-hidden="true"><Icon size={14} /></span>
    </div>
    <div class="timeline-card">
      <div class="timeline-topline">
        <div class="timeline-target">
          {#if showHost && event.host}<a href={event.host_id ? hostHref(event.host_id) : "/"} class="timeline-host">{event.host}</a>{/if}
          {#if showHost && event.host && event.service}<span class="text-fg-3" aria-hidden="true">/</span>{/if}
          {#if event.service && event.host_id}<a href={serviceHref(event.host_id, event.service)} class="timeline-service">{event.service}</a>{:else if event.service}<span class="timeline-service">{event.service}</span>{/if}
          {#if event.source === "monarch"}<span class="timeline-source">monarch</span>{/if}
        </div>
        <span class="timeline-state">{eventStateLabel[event.state]}</span>
      </div>
      <button type="button" class="timeline-message" aria-label="View event details: {message}" onclick={() => (detailsOpen = true)}>{message}</button>
      <div class="timeline-actions">
        {#if event.acked_by}<span class="timeline-ack"><Check size={12} /> Acknowledged by {event.acked_by}</span>{/if}
        <button type="button" class="timeline-details" aria-label="View event details: {message}" onclick={() => (detailsOpen = true)}><ArrowUpRight size={13} aria-hidden="true" /> Details</button>
        {#if onack && !event.acked_by}<button type="button" class="btn btn-sm" onclick={onack}><Check size={13} /> Acknowledge</button>{/if}
      </div>
    </div>
  </article>
  <Modal bind:open={detailsOpen} title="Event details" width="max-w-xl">
    <div class="event-details">
      <div class="event-details-state tone-{tone}"><span class="timeline-marker"><Icon size={15} /></span>{eventStateLabel[event.state]}</div>
      <dl>
        <dt>When</dt><dd>{datetime(event.created_at)}</dd>
        <dt>State</dt><dd>{eventStateLabel[event.state]}</dd>
        <dt>Host</dt><dd>{#if event.host_id !== null && event.host}<a href={hostHref(event.host_id)}>{event.host}</a>{:else}{event.host ?? "—"}{/if}</dd>
        <dt>Service</dt><dd>{#if event.host_id !== null && event.service}<a href={serviceHref(event.host_id, event.service)}>{event.service}</a>{:else}{event.service ?? "—"}{/if}</dd>
        <dt>Event</dt><dd>{event.kind_label || event.kind}</dd>
        {#if event.acked_by}<dt>Acknowledged</dt><dd>{event.acked_by}{event.acked_at ? ` · ${datetime(event.acked_at)}` : ""}</dd>{/if}
      </dl>
      <p class="event-full-message">{message}</p>
      {#if onack && !event.acked_by}<button class="btn btn-primary mt-4" type="button" onclick={() => { onack?.(); detailsOpen = false; }}><Check size={15} /> Acknowledge event</button>{/if}
    </div>
  </Modal>
{:else}
<div class="group flex gap-3 {compact ? 'px-4 py-2.5' : 'px-4 py-3.5 sm:px-5'} transition-colors hover:bg-hover {event.acked_by ? 'opacity-70' : ''}">
  {#if selectable}
    <input
      type="checkbox"
      class="mt-1 h-4 w-4 shrink-0 accent-[var(--mn-accent)]"
      checked={selected}
      onchange={() => ontoggle?.()}
      aria-label="Select event"
    />
  {/if}
  <div class="tone-{tone} mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-tone-soft text-tone">
    <Icon size={15} />
  </div>
  <div class="min-w-0 flex-1">
    <div class="flex min-w-0 items-center gap-x-2 text-[13px]">
      {#if showHost && event.host}
        <a href={event.host_id ? hostHref(event.host_id) : "/"} class="shrink-0 font-semibold text-fg hover:underline">{event.host}</a>
        {#if event.service}<span class="text-fg-3">/</span>{/if}
      {/if}
      {#if event.service && event.host_id}
        <a href={serviceHref(event.host_id, event.service)} class="truncate font-medium text-fg-2 hover:text-fg hover:underline">{event.service}</a>
      {/if}
      {#if event.source === "monarch"}
        <span class="shrink-0 rounded-md border border-line px-1.5 py-px text-[10px] font-medium text-fg-3">monarch</span>
      {/if}
    </div>
    <div class="mt-0.5 flex items-baseline gap-2 text-[13px] leading-snug text-fg-2">
      <span
        class="tone-{tone} shrink-0 rounded-md bg-tone-soft px-1.5 py-px text-[10px] font-semibold tracking-wide text-tone uppercase"
        title={event.kind_label}>{eventStateLabel[event.state]}</span
      >
      <span class="min-w-0 break-words {compact ? 'line-clamp-1' : 'line-clamp-3'}">{message}</span>
    </div>
    {#if !compact && event.acked_by}
      <div class="mt-1 flex items-center gap-1 text-[11px] text-fg-3"><Check size={12} /> acknowledged by {event.acked_by}</div>
    {/if}
  </div>
  <div class="flex shrink-0 flex-col items-end gap-1.5">
    <time class="num text-[11px] whitespace-nowrap text-fg-3" title={datetime(event.created_at)} datetime={new Date(event.created_at * 1000).toISOString()}
      >{ago(event.created_at, clock.now)}</time
    >
    {#if onack && !event.acked_by}
      <button class="btn btn-sm opacity-0 transition-opacity group-hover:opacity-100 focus:opacity-100 max-sm:opacity-100" onclick={onack}>
        <Check size={13} /> Ack
      </button>
    {/if}
  </div>
</div>
{/if}

<style>
  .timeline-row { display: grid; grid-template-columns: 126px minmax(0, 1fr); gap: 14px; position: relative; padding: 8px 14px; }
  .timeline-gutter { position: relative; display: flex; align-items: flex-start; justify-content: flex-end; padding-top: 15px; padding-right: 34px; }
  .timeline-gutter::after { position: absolute; top: 0; bottom: -16px; right: 10px; width: 1px; background: var(--mn-line); content: ""; }
  .timeline-row:last-child .timeline-gutter::after { display: none; }
  .timeline-time { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; color: var(--mn-fg-2); font-size: 13px; white-space: nowrap; }
  .timeline-time span { color: var(--mn-fg-3); font-size: 11px; }
  .timeline-marker { position: absolute; z-index: 1; top: 17px; right: 0; display: grid; width: 22px; height: 22px; place-items: center; border: 2px solid var(--mn-surface); border-radius: 50%; background: color-mix(in oklab, var(--tone) 18%, var(--mn-surface)); color: var(--tone); }
  .timeline-card { min-width: 0; border: 1px solid var(--mn-line); border-radius: 10px; background: var(--mn-surface-2); padding: 11px 13px; }
  .timeline-row.tone-bad { --tone: var(--mn-bad); }
  .timeline-row.tone-ok { --tone: var(--mn-ok); }
  .timeline-row.tone-info { --tone: var(--mn-info); }
  .timeline-row.tone-muted { --tone: var(--mn-fg-3); }
  .timeline-topline { display: flex; min-width: 0; align-items: center; justify-content: space-between; gap: 10px; }
  .timeline-target { display: flex; min-width: 0; align-items: center; gap: 6px; flex-wrap: wrap; font-size: 14px; }
  .timeline-host, .timeline-service { color: var(--mn-fg); font-weight: 600; overflow-wrap: anywhere; }
  .timeline-service { color: var(--mn-fg-2); }
  .timeline-source { border: 1px solid var(--mn-line); border-radius: 5px; padding: 1px 5px; color: var(--mn-fg-3); font-size: 10px; }
  .timeline-state { flex: none; color: var(--tone); font-size: 12px; font-weight: 650; }
  .timeline-message { display: block; width: 100%; margin-top: 5px; border: 0; background: transparent; padding: 0; color: var(--mn-fg-2); font: inherit; font-size: 14px; line-height: 1.45; text-align: left; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; cursor: pointer; }
  .timeline-message:hover { color: var(--mn-fg); }
  .timeline-actions { display: flex; min-height: 24px; align-items: center; justify-content: flex-end; gap: 9px; margin-top: 5px; }
  .timeline-ack { display: flex; align-items: center; gap: 4px; margin-right: auto; color: var(--mn-fg-3); font-size: 11px; }
  .timeline-details { display: inline-flex; align-items: center; gap: 4px; border: 0; background: none; padding: 4px 2px; color: var(--mn-fg-3); font-size: 12px; cursor: pointer; }
  .timeline-details:hover { color: var(--mn-fg); text-decoration: underline; }
  .timeline-details:focus-visible, .timeline-row a:focus-visible { outline: 2px solid var(--mn-accent); outline-offset: 2px; }
  .timeline-select { position: absolute; z-index: 2; top: 20px; left: 0; accent-color: var(--mn-accent); }
  .timeline-acked .timeline-card { opacity: .72; }
  .event-details-state { display: flex; align-items: center; gap: 10px; color: var(--tone); font-weight: 650; }
  .event-details-state.tone-bad { --tone: var(--mn-bad); }
  .event-details-state.tone-ok { --tone: var(--mn-ok); }
  .event-details-state.tone-info { --tone: var(--mn-info); }
  .event-details-state.tone-muted { --tone: var(--mn-fg-3); }
  .event-details-state .timeline-marker { position: static; border-color: var(--mn-surface-2); }
  .event-details dl { display: grid; grid-template-columns: 100px minmax(0, 1fr); gap: 8px 12px; margin: 18px 0; font-size: 14px; }
  .event-details dt { color: var(--mn-fg-3); }
  .event-details dd { min-width: 0; margin: 0; color: var(--mn-fg-2); overflow-wrap: anywhere; }
  .event-details a { color: var(--mn-accent); text-decoration: underline; text-underline-offset: 2px; }
  .event-full-message { border: 1px solid var(--mn-line); border-radius: 9px; background: var(--mn-surface-2); padding: 12px; color: var(--mn-fg); font-size: 14px; line-height: 1.5; overflow-wrap: anywhere; white-space: pre-wrap; }
  @media (max-width: 640px) {
    .timeline-row { grid-template-columns: 24px minmax(0, 1fr); gap: 9px; padding: 6px 9px; }
    .timeline-gutter { justify-content: flex-start; padding-top: 8px; padding-right: 0; }
    .timeline-gutter::after { right: auto; left: 10px; }
    .timeline-time { position: absolute; top: 7px; left: 40px; display: flex; align-items: baseline; flex-direction: row; gap: 7px; font-size: 12px; }
    .timeline-marker { top: 10px; right: auto; left: 0; }
    .timeline-select { top: 40px; left: 2px; }
    .timeline-card { margin-top: 26px; padding: 10px; }
    .timeline-topline { align-items: flex-start; }
    .timeline-state { font-size: 11px; }
  }
</style>
