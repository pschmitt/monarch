<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";
  let {
    open = $bindable(false),
    title = "",
    width = "max-w-lg",
    onclose,
    children,
    footer,
  }: {
    open?: boolean;
    title?: string;
    width?: string;
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  function close() {
    open = false;
    onclose?.();
  }
  function onkey(e: KeyboardEvent) {
    if (open && e.key === "Escape") {
      e.stopPropagation();
      close();
    }
  }
</script>

<svelte:window onkeydown={onkey} />

{#if open}
  <div class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-6" role="presentation">
    <button class="absolute inset-0 cursor-default bg-[var(--scrim)] backdrop-blur-sm" style="animation: fade-up 160ms both" aria-label="Close dialog" onclick={close}></button>
    <div
      role="dialog"
      aria-modal="true"
      aria-label={title}
      class="relative w-full {width} max-h-[92dvh] overflow-hidden rounded-t-2xl border border-line-strong bg-surface-solid shadow-2xl sm:rounded-2xl animate-in flex flex-col"
    >
      {#if title}
        <div class="flex items-center justify-between gap-4 border-b border-line px-5 py-4">
          <h2 class="text-[15px] font-semibold text-fg">{title}</h2>
          <button class="btn btn-ghost btn-sm btn-icon" onclick={close} aria-label="Close"><X size={16} /></button>
        </div>
      {/if}
      <div class="overflow-y-auto px-5 py-4">{@render children()}</div>
      {#if footer}
        <div class="flex items-center justify-end gap-2 border-t border-line bg-[var(--surface-hover)] px-5 py-3">{@render footer()}</div>
      {/if}
    </div>
  </div>
{/if}
