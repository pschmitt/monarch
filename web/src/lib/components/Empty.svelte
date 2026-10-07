<script lang="ts">
  import type { Component, Snippet } from "svelte";
  let {
    icon: Icon,
    title,
    body = "",
    children,
  }: { icon: Component<any>; title: string; body?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center justify-center px-6 py-14 text-center animate-in">
  <div class="relative mb-4">
    <div class="absolute inset-0 -m-3 rounded-full bg-accent-gradient opacity-20 blur-xl"></div>
    <div class="relative flex h-12 w-12 items-center justify-center rounded-2xl border border-line-strong bg-surface-2 text-fg-2">
      <Icon size={22} strokeWidth={1.75} />
    </div>
  </div>
  <div class="text-sm font-semibold text-fg">{title}</div>
  {#if body}<div class="mt-1 max-w-sm text-[13px] text-fg-3">{body}</div>{/if}
  {#if children}<div class="mt-4">{@render children()}</div>{/if}
</div>
