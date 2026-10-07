<script lang="ts">
  import type { Snippet } from "svelte";
  let { label, mono = false, children }: { label: string; mono?: boolean; children: Snippet } = $props();
</script>

<div class="flex min-w-0 flex-col gap-1">
  <dt class="eyebrow">{label}</dt>
  <dd class="truncate text-[13px] text-fg {mono ? 'num' : ''}">{@render children()}</dd>
</div>
