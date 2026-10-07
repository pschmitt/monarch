<script lang="ts">
  import type { Tone } from "../format";
  let { tone = "muted", pulse = false, size = 8 }: { tone?: Tone; pulse?: boolean; size?: number } = $props();
</script>

<span class="relative inline-flex shrink-0 tone-{tone}" style="width:{size}px;height:{size}px">
  {#if pulse}
    <span class="absolute inset-0 rounded-full bg-tone" style="animation: ping-ring 1.8s cubic-bezier(0,0,0.2,1) infinite"></span>
  {/if}
  <span class="relative inline-block rounded-full bg-tone glow-tone" style="width:{size}px;height:{size}px"></span>
</span>
