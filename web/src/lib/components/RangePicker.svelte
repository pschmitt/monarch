<script lang="ts" module>
  export const RANGES = [
    { id: "1h", label: "1h", seconds: 3600 },
    { id: "6h", label: "6h", seconds: 6 * 3600 },
    { id: "24h", label: "24h", seconds: 86400 },
    { id: "7d", label: "7d", seconds: 7 * 86400 },
    { id: "30d", label: "30d", seconds: 30 * 86400 },
  ] as const;
  export type RangeId = (typeof RANGES)[number]["id"];
  export const rangeSeconds = (id: string) => RANGES.find((r) => r.id === id)?.seconds ?? 6 * 3600;
</script>

<script lang="ts">
  let { value = $bindable("6h") }: { value?: string } = $props();
</script>

<div class="inline-flex rounded-xl border border-line bg-surface-2 p-0.5" role="radiogroup" aria-label="Time range">
  {#each RANGES as r (r.id)}
    <button
      role="radio"
      aria-checked={value === r.id}
      class="num h-7 rounded-[9px] px-2.5 text-[11px] font-medium transition-all {value === r.id
        ? 'bg-surface-solid text-fg shadow-sm ring-1 ring-[var(--line-strong)]'
        : 'text-fg-3 hover:text-fg-2'}"
      onclick={() => (value = r.id)}>{r.label}</button
    >
  {/each}
</div>
