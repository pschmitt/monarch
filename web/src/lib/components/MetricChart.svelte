<script lang="ts">
  import type { Snippet } from "svelte";
  import { api } from "../api";
  import Chart, { type ChartSeries } from "./Chart.svelte";
  import { rangeSeconds } from "./RangePicker.svelte";
  import { ChartNoAxesColumn } from "@lucide/svelte";

  interface MetricDef {
    key: string;
    label: string;
    color: string;
    fill?: boolean;
    dashed?: boolean;
  }

  let {
    host,
    service,
    metrics,
    range = "6h",
    title,
    format = (v: number) => v.toFixed(1),
    stacked = false,
    yMax = null,
    height = 190,
    current,
    actions,
  }: {
    host: number;
    service: string;
    metrics: MetricDef[];
    range?: string;
    title: string;
    format?: (v: number) => string;
    stacked?: boolean;
    yMax?: number | null;
    height?: number;
    current?: string | null;
    actions?: Snippet;
  } = $props();

  let data = $state<{ x: number[]; series: ChartSeries[] } | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let span = $state(6 * 3600);

  async function load() {
    const secs = rangeSeconds(range);
    const to = Math.floor(Date.now() / 1000);
    try {
      const res = await api.metrics({ host, service, metrics: metrics.map((m) => m.key), from: to - secs, to });
      const xs = new Set<number>();
      for (const s of res.series) for (const p of s.points) xs.add(p[0]);
      const x = [...xs].sort((a, b) => a - b);
      const index = new Map(x.map((t, i) => [t, i]));
      const series = metrics.map((m) => {
        const vals: (number | null)[] = x.map(() => null);
        const s = res.series.find((r) => r.metric === m.key);
        if (s) for (const p of s.points) vals[index.get(p[0])!] = p[1];
        return { label: m.label, color: m.color, values: vals, fill: m.fill, dashed: m.dashed };
      });
      span = secs;
      data = { x, series };
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void host;
    void service;
    void range;
    loading = true;
    load();
    const t = setInterval(load, 60_000);
    return () => clearInterval(t);
  });

  const hasData = $derived(!!data && data.x.length > 1 && data.series.some((s) => s.values.some((v) => v !== null)));
</script>

<div class="card flex min-w-0 flex-col p-4 sm:p-5">
  <div class="mb-3 flex items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="card-title">{title}</div>
      <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1">
        {#each metrics as m (m.key)}
          <span class="flex items-center gap-1.5 text-[11px] text-fg-3">
            <span class="h-1.5 w-3 rounded-full" style="background:{m.color}"></span>{m.label}
          </span>
        {/each}
      </div>
    </div>
    <div class="flex items-center gap-2">
      {#if current}<span class="num text-lg font-semibold text-fg">{current}</span>{/if}
      {#if actions}{@render actions()}{/if}
    </div>
  </div>
  {#if loading && !data}
    <div class="skeleton w-full" style="height:{height}px"></div>
  {:else if error}
    <div class="flex items-center justify-center text-xs text-bad" style="height:{height}px">{error}</div>
  {:else if !hasData}
    <div class="flex flex-col items-center justify-center gap-2 text-xs text-fg-3" style="height:{height}px">
      <ChartNoAxesColumn size={20} strokeWidth={1.5} />No data for this range yet
    </div>
  {:else if data}
    <Chart x={data.x} series={data.series} {format} {stacked} {yMax} {height} {span} />
  {/if}
</div>
