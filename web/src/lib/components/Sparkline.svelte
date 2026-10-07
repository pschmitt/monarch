<script lang="ts">
  let {
    values,
    color = "var(--accent)",
    height = 32,
    max = null,
    fill = true,
  }: { values: number[]; color?: string; height?: number; max?: number | null; fill?: boolean } = $props();
  const id = `sp-${Math.random().toString(36).slice(2, 8)}`;
  const W = 100;
  const path = $derived.by(() => {
    const v = values.filter((x) => Number.isFinite(x));
    if (v.length < 2) return { line: "", area: "" };
    const hi = max ?? Math.max(...v, 1);
    const lo = max !== null ? 0 : Math.min(...v, 0);
    const span = hi - lo || 1;
    const pts = v.map((x, i) => [(i / (v.length - 1)) * W, height - 2 - ((x - lo) / span) * (height - 4)]);
    const line = pts.map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(2)},${p[1].toFixed(2)}`).join(" ");
    return { line, area: `${line} L${W},${height} L0,${height} Z` };
  });
</script>

<svg viewBox="0 0 {W} {height}" preserveAspectRatio="none" class="block w-full" style="height:{height}px" aria-hidden="true">
  <defs>
    <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color={color} stop-opacity="0.32" />
      <stop offset="1" stop-color={color} stop-opacity="0" />
    </linearGradient>
  </defs>
  {#if fill && path.area}<path d={path.area} fill="url(#{id})" />{/if}
  {#if path.line}
    <path d={path.line} fill="none" stroke={color} stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linejoin="round" stroke-linecap="round" />
  {/if}
</svg>
