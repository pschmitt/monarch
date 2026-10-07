<script lang="ts">
  import { ArrowRightLeft, CircleCheck, CircleX, HeartPulse, Check, Circle } from "@lucide/svelte";
  import type { MonarchEvent } from "../types";
  import { ago, datetime, eventTone } from "../format";
  import { clock } from "../state.svelte";
  import { hostHref, serviceHref } from "../router.svelte";

  let {
    event,
    compact = false,
    showHost = true,
    selectable = false,
    selected = false,
    ontoggle,
    onack,
  }: {
    event: MonarchEvent;
    compact?: boolean;
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
</script>

<div class="group flex gap-3 {compact ? 'px-4 py-2.5' : 'px-4 py-3.5 sm:px-5'} transition-colors hover:bg-hover {event.acked_by ? 'opacity-70' : ''}">
  {#if selectable}
    <input
      type="checkbox"
      class="mt-1 h-4 w-4 shrink-0 accent-[var(--accent)]"
      checked={selected}
      onchange={() => ontoggle?.()}
      aria-label="Select event"
    />
  {/if}
  <div class="tone-{tone} mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-tone-soft text-tone">
    <Icon size={15} />
  </div>
  <div class="min-w-0 flex-1">
    <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[13px]">
      {#if showHost && event.host}
        <a href={event.host_id ? hostHref(event.host_id) : "/"} class="font-semibold text-fg hover:underline">{event.host}</a>
        {#if event.service}<span class="text-fg-3">/</span>{/if}
      {/if}
      {#if event.service && event.host_id}
        <a href={serviceHref(event.host_id, event.service)} class="font-medium text-fg-2 hover:text-fg hover:underline">{event.service}</a>
      {/if}
      <span class="tone-{tone} rounded-md bg-tone-soft px-1.5 py-px text-[10px] font-semibold tracking-wide text-tone uppercase">{event.kind_label}</span>
      {#if event.source === "monarch"}
        <span class="rounded-md border border-line px-1.5 py-px text-[10px] font-medium text-fg-3">monarch</span>
      {/if}
    </div>
    <div class="mt-0.5 text-[13px] leading-snug break-words text-fg-2 {compact ? 'line-clamp-1' : 'line-clamp-3'}">{event.message}</div>
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
