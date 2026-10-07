<script lang="ts">
  import { BellOff, Clock } from "@lucide/svelte";
  import type { HostSummary } from "../types";
  import { ago, duration, hostName, hostStateLabel, hostTone, kb, osLabel, pct } from "../format";
  import { clock } from "../state.svelte";
  import { hostHref } from "../router.svelte";
  import Gauge from "./Gauge.svelte";
  import Sparkline from "./Sparkline.svelte";
  import StatusDot from "./StatusDot.svelte";

  let { host }: { host: HostSummary } = $props();
  const tone = $derived(hostTone(host.state));
  const muted = $derived(!!host.muted_until && host.muted_until > clock.now);
</script>

<a
  href={hostHref(host.id)}
  class="card card-hover group relative flex flex-col overflow-hidden p-4 tone-{tone} {host.state === 'offline' ? 'opacity-80' : ''}"
>
  {#if host.state !== "ok"}
    <div class="pointer-events-none absolute inset-x-0 top-0 h-px" style="background: linear-gradient(90deg, transparent, var(--tone), transparent)"></div>
    <div class="pointer-events-none absolute -top-16 left-1/2 h-24 w-2/3 -translate-x-1/2 rounded-full bg-tone opacity-[0.08] blur-2xl"></div>
  {/if}
  <div class="flex items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <StatusDot {tone} pulse={host.state !== "ok"} />
        <span class="truncate text-[15px] font-semibold tracking-tight text-fg">{hostName(host)}</span>
        {#if muted}<BellOff size={13} class="text-fg-3" />{/if}
      </div>
      <div class="mt-1 truncate text-xs text-fg-3">{osLabel(host.os)}</div>
    </div>
    <span class="text-[11px] font-medium text-tone">{hostStateLabel[host.state]}</span>
  </div>

  <div class="mt-4 flex items-center gap-4">
    <Gauge value={host.system.cpu} label="cpu" size={58} stroke={5} />
    <Gauge value={host.system.mem_percent} label="mem" size={58} stroke={5} />
    <div class="min-w-0 flex-1 space-y-2">
      <div>
        <div class="flex items-center justify-between text-[10px] text-fg-3"><span>CPU 1h</span><span class="num">{pct(host.system.cpu)}</span></div>
        <Sparkline values={host.sparkline.cpu} max={100} height={20} color="var(--accent)" />
      </div>
      <div>
        <div class="flex items-center justify-between text-[10px] text-fg-3"><span>Memory</span><span class="num">{kb(host.system.mem_kb, 1)}</span></div>
        <Sparkline values={host.sparkline.mem} max={100} height={20} color="var(--accent-2)" />
      </div>
    </div>
  </div>

  <div class="mt-4 flex items-center justify-between gap-2 border-t border-line pt-3 text-[11px] text-fg-3">
    <span class="num">
      <span class="text-ok">{host.services.ok}</span><span class="opacity-50"> / </span>{host.services.total} services
    </span>
    {#if host.online}
      <span class="num flex items-center gap-1"><Clock size={11} /> up {duration(host.system.uptime, 1)}</span>
    {:else}
      <span class="num text-bad">seen {ago(host.last_seen, clock.now)}</span>
    {/if}
  </div>

  {#if host.failing.length}
    <div class="mt-3 flex flex-wrap gap-1.5">
      {#each host.failing.slice(0, 3) as f (f)}
        <span title={f} class="tone-bad inline-flex max-w-full items-center gap-1 truncate rounded-md border border-tone-soft bg-tone-soft px-1.5 py-0.5 text-[11px] font-medium text-tone">{f}</span>
      {/each}
      {#if host.failing.length > 3}<span class="text-[11px] text-fg-3">+{host.failing.length - 3}</span>{/if}
    </div>
  {/if}
</a>
