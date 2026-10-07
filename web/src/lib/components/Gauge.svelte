<script lang="ts">
  import { usageTone, type Tone } from "../format";
  let {
    value,
    label = "",
    size = 64,
    stroke = 6,
    tone,
  }: { value: number | null; label?: string; size?: number; stroke?: number; tone?: Tone } = $props();
  const r = $derived((size - stroke) / 2);
  const c = $derived(2 * Math.PI * r);
  const pct = $derived(value === null ? 0 : Math.max(0, Math.min(100, value)));
  const t = $derived(tone ?? usageTone(value));
</script>

<div class="relative inline-flex items-center justify-center tone-{t}" style="width:{size}px;height:{size}px">
  <svg width={size} height={size} class="-rotate-90">
    <circle cx={size / 2} cy={size / 2} {r} fill="none" stroke="var(--line)" stroke-width={stroke} />
    <circle
      cx={size / 2}
      cy={size / 2}
      {r}
      fill="none"
      stroke={t === "ok" ? "url(#gauge-ok)" : "var(--tone)"}
      stroke-width={stroke}
      stroke-linecap="round"
      stroke-dasharray={c}
      stroke-dashoffset={c * (1 - pct / 100)}
      style="transition: stroke-dashoffset 600ms cubic-bezier(.2,.7,.2,1)"
    />
    <defs>
      <linearGradient id="gauge-ok" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="var(--accent)" />
        <stop offset="1" stop-color="var(--accent-2)" />
      </linearGradient>
    </defs>
  </svg>
  <div class="absolute inset-0 flex flex-col items-center justify-center leading-none">
    <span class="num text-[13px] font-semibold text-fg">{value === null ? "—" : `${Math.round(value)}`}<span class="text-[9px] text-fg-3">{value === null ? "" : "%"}</span></span>
    {#if label}<span class="mt-0.5 text-[9px] font-semibold tracking-wider text-fg-3 uppercase">{label}</span>{/if}
  </div>
</div>
