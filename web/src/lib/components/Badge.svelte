<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Tone } from "../format";
  import StatusDot from "./StatusDot.svelte";
  let {
    tone = "muted",
    dot = false,
    pulse = false,
    size = "md",
    children,
  }: { tone?: Tone; dot?: boolean; pulse?: boolean; size?: "sm" | "md"; children: Snippet } = $props();
</script>

<span
  class="tone-{tone} inline-flex items-center gap-1.5 rounded-full border border-tone-soft bg-tone-soft font-medium text-tone whitespace-nowrap
    {size === 'sm' ? 'h-5 px-2 text-[11px]' : 'h-6 px-2.5 text-xs'}"
>
  {#if dot}<StatusDot {tone} {pulse} size={6} />{/if}
  {@render children()}
</span>
