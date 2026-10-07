<script lang="ts">
  import { usageTone, type Tone } from "../format";
  let { value, tone, height = 6 }: { value: number | null | undefined; tone?: Tone; height?: number } = $props();
  const pct = $derived(value === null || value === undefined ? 0 : Math.max(0, Math.min(100, value)));
  const t = $derived(tone ?? usageTone(value));
</script>

<div class="w-full overflow-hidden rounded-full bg-[var(--line)] tone-{t}" style="height:{height}px">
  <div
    class="h-full rounded-full {t === 'ok' ? 'bg-accent-gradient' : 'bg-tone'}"
    style="width:{pct}%; transition: width 600ms cubic-bezier(.2,.7,.2,1)"
  ></div>
</div>
