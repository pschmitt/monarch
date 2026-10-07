<script lang="ts" module>
  export interface ChartSeries {
    label: string;
    values: (number | null)[];
    color: string;
    fill?: boolean;
    dashed?: boolean;
  }
</script>

<script lang="ts">
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  import { onDestroy } from "svelte";
  import { theme } from "../state.svelte";

  let {
    x,
    series,
    format = (v: number) => v.toFixed(1),
    height = 200,
    stacked = false,
    yMax = null,
    yMin = 0,
    span = 0,
  }: {
    x: number[];
    series: ChartSeries[];
    format?: (v: number) => string;
    height?: number;
    stacked?: boolean;
    yMax?: number | null;
    yMin?: number | null;
    /** time span in seconds, used for axis label formatting */
    span?: number;
  } = $props();

  let el: HTMLDivElement;
  let plot: uPlot | null = null;
  let tip = $state<{ left: number; top: number; idx: number; flip: boolean } | null>(null);

  function cssVar(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
  }

  function resolveColor(c: string): string {
    const m = c.match(/^var\((--[^)]+)\)$/);
    return m ? cssVar(m[1]) : c;
  }

  function gradientFill(color: string, top: number, bottom: number) {
    return (u: uPlot) => {
      const g = u.ctx.createLinearGradient(0, u.bbox.top, 0, u.bbox.top + u.bbox.height);
      g.addColorStop(0, toRgba(color, top));
      g.addColorStop(1, toRgba(color, bottom));
      return g;
    };
  }

  // Canvas gradients don't accept color-mix(), so convert via a probe element.
  const rgbaCache = new Map<string, [number, number, number]>();
  function toRgba(color: string, alpha: number): string {
    let rgb = rgbaCache.get(color);
    if (!rgb) {
      const c = document.createElement("canvas").getContext("2d")!;
      c.fillStyle = color;
      const hex = c.fillStyle as string;
      if (hex.startsWith("#")) {
        rgb = [parseInt(hex.slice(1, 3), 16), parseInt(hex.slice(3, 5), 16), parseInt(hex.slice(5, 7), 16)];
      } else {
        const m = hex.match(/\d+(\.\d+)?/g) ?? ["128", "128", "128"];
        rgb = [+m[0], +m[1], +m[2]];
      }
      rgbaCache.set(color, rgb);
    }
    return `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${alpha})`;
  }

  // Break lines where samples are missing (agent down, Monarch restarted, ...)
  // instead of interpolating across the gap.
  const gapped = $derived.by(() => {
    const steps = x.slice(1).map((t, i) => t - x[i]).sort((a, b) => a - b);
    const step = steps.length ? steps[Math.floor(steps.length / 2)] : 0;
    const xs: number[] = [];
    const vals: (number | null)[][] = series.map(() => []);
    x.forEach((t, i) => {
      if (i > 0 && step > 0 && t - x[i - 1] > Math.max(step * 3, 300)) {
        xs.push(x[i - 1] + step);
        vals.forEach((v) => v.push(null));
      }
      xs.push(t);
      series.forEach((sr, j) => vals[j].push(sr.values[i] ?? null));
    });
    return { x: xs, values: vals };
  });

  const plotData = $derived.by(() => {
    const { values } = gapped;
    if (!stacked) return values;
    // cumulative sums, drawn top-first so lower layers overlay
    const acc: (number | null)[][] = [];
    let run: (number | null)[] = gapped.x.map(() => 0);
    for (const v of values) {
      run = run.map((r, i) => (values.every((w) => w[i] === null) ? null : (r ?? 0) + (v[i] ?? 0)));
      acc.push([...run]);
    }
    return acc;
  });

  function timeFmt(ts: number, range: number): string {
    const d = new Date(ts * 1000);
    if (span > 2 * 86400) return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
    if (range < 15 * 60) return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }

  function build() {
    plot?.destroy();
    plot = null;
    if (!el || gapped.x.length === 0) return;
    const axis = cssVar("--chart-axis");
    const grid = cssVar("--chart-grid");
    const font = `11px ${cssVar("--font-mono") || "monospace"}`;
    const ordered = stacked ? [...series.keys()].reverse() : [...series.keys()];

    const opts: uPlot.Options = {
      width: el.clientWidth || 600,
      height,
      padding: [8, 4, 0, 0],
      cursor: {
        points: { size: 7, width: 2, fill: cssVar("--surface-solid") },
        drag: { x: false, y: false },
        y: false,
      },
      legend: { show: false },
      scales: {
        x: { time: true },
        y: {
          range: (_u, dmin, dmax) => {
            const lo = yMin ?? dmin;
            let hi = yMax ?? dmax;
            if (yMax === null) hi = dmax <= lo ? lo + 1 : dmax + (dmax - lo) * 0.12;
            return [lo, hi];
          },
        },
      },
      axes: [
        {
          stroke: axis,
          font,
          grid: { show: false },
          ticks: { show: false },
          gap: 6,
          size: 28,
          space: 90,
          values: (_u, vals) => {
            const range = vals.length > 1 ? vals[vals.length - 1] - vals[0] : 0;
            return vals.map((v) => timeFmt(v, range));
          },
        },
        {
          stroke: axis,
          font,
          grid: { stroke: grid, width: 1 },
          ticks: { show: false },
          gap: 8,
          size: (u, values) => {
            // Fit the widest tick label.
            if (!values?.length) return 40;
            const ctx = u.ctx;
            const prev = ctx.font;
            ctx.font = font;
            const w = Math.max(...values.map((v) => ctx.measureText(String(v)).width));
            ctx.font = prev;
            return Math.ceil(w + 14);
          },
          values: (_u, vals) => vals.map((v) => format(v)),
        },
      ],
      series: [
        {},
        ...ordered.map((i) => {
          const s = series[i];
          const color = resolveColor(s.color);
          return {
            label: s.label,
            stroke: color,
            width: 1.75,
            dash: s.dashed ? [4, 4] : undefined,
            fill: s.fill === false ? undefined : stacked ? toRgba(color, 0.55) : gradientFill(color, 0.28, 0.0),
            points: { show: false },
            spanGaps: false,
          } satisfies uPlot.Series;
        }),
      ],
      hooks: {
        setCursor: [
          (u) => {
            const { left, top, idx } = u.cursor;
            if (idx === null || idx === undefined || left === undefined || left < 0) {
              tip = null;
              return;
            }
            tip = { left: left! + u.bbox.left / devicePixelRatio, top: top ?? 0, idx, flip: left! > u.bbox.width / devicePixelRatio / 2 };
          },
        ],
      },
    };
    const data = [gapped.x, ...ordered.map((i) => plotData[i])] as uPlot.AlignedData;
    plot = new uPlot(opts, data, el);
  }

  $effect(() => {
    // rebuild on structural changes + theme switch
    void theme.value;
    void series.length;
    void stacked;
    void height;
    build();
  });

  $effect(() => {
    if (!plot) return;
    const ordered = stacked ? [...series.keys()].reverse() : [...series.keys()];
    plot.setData([gapped.x, ...ordered.map((i) => plotData[i])] as uPlot.AlignedData);
  });

  $effect(() => {
    if (!el) return;
    const ro = new ResizeObserver(() => {
      if (plot && el.clientWidth) plot.setSize({ width: el.clientWidth, height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  onDestroy(() => plot?.destroy());

  function fullTime(ts: number) {
    return new Date(ts * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="relative min-w-0" onmouseleave={() => (tip = null)} role="img" aria-label="Chart">
  <div bind:this={el} class="w-full" style="min-height:{height}px"></div>
  {#if tip && gapped.x[tip.idx] !== undefined}
    <div
      class="pointer-events-none absolute z-10 min-w-36 rounded-xl border border-line-strong bg-surface-solid/95 px-3 py-2 shadow-xl backdrop-blur"
      style="top: 8px; {tip.flip ? `right: calc(100% - ${tip.left}px + 14px)` : `left: ${tip.left + 14}px`}"
    >
      <div class="num mb-1.5 text-[10px] text-fg-3">{fullTime(gapped.x[tip.idx])}</div>
      {#each series as s, i (i)}
        {@const v = gapped.values[i]?.[tip.idx]}
        <div class="flex items-center justify-between gap-4 text-xs">
          <span class="flex items-center gap-1.5 text-fg-2">
            <span class="h-2 w-2 rounded-full" style="background:{s.color}"></span>{s.label}
          </span>
          <span class="num font-medium text-fg">{v === null || v === undefined ? "—" : format(v)}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>
