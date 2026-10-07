<script lang="ts" module>
  export interface MenuItem {
    label: string;
    icon?: import("svelte").Component<any>;
    danger?: boolean;
    disabled?: boolean;
    onselect: () => void;
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  let {
    items,
    align = "right",
    trigger,
    label = "Open menu",
  }: { items: (MenuItem | "sep")[]; align?: "left" | "right"; trigger: Snippet; label?: string } = $props();
  let open = $state(false);
  let root: HTMLDivElement;
  let button: HTMLButtonElement;
  let panel = $state<HTMLDivElement>();
  let pos = $state({ top: 0, left: 0 });

  // The panel lives on <body> so cards with overflow/backdrop-filter cannot clip it.
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function place() {
    if (!button || !panel) return;
    const r = button.getBoundingClientRect();
    const w = panel.offsetWidth;
    const h = panel.offsetHeight;
    let left = align === "right" ? r.right - w : r.left;
    left = Math.max(8, Math.min(left, window.innerWidth - w - 8));
    let top = r.bottom + 6;
    if (top + h > window.innerHeight - 8) top = Math.max(8, r.top - h - 6);
    pos = { top, left };
  }

  $effect(() => {
    if (open && panel) place();
  });

  function onwin(e: MouseEvent) {
    const t = e.target as Node;
    if (open && root && !root.contains(t) && !panel?.contains(t)) open = false;
  }
</script>

<svelte:window onclick={onwin} onkeydown={(e) => e.key === "Escape" && (open = false)} onresize={() => (open = false)} />

<div class="relative inline-block" bind:this={root}>
  <button bind:this={button} class="btn btn-ghost btn-sm btn-icon" aria-haspopup="menu" aria-expanded={open} aria-label={label} onclick={() => (open = !open)}>
    {@render trigger()}
  </button>
  {#if open}
    <div
      use:portal
      bind:this={panel}
      role="menu"
      style="position: fixed; top: {pos.top}px; left: {pos.left}px"
      class="z-[60] min-w-44 overflow-hidden rounded-xl border border-line-strong bg-surface-solid p-1 shadow-2xl animate-in"
    >
      {#each items as item, i (i)}
        {#if item === "sep"}
          <div class="my-1 h-px bg-[var(--line)]"></div>
        {:else}
          <button
            role="menuitem"
            disabled={item.disabled}
            class="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-[13px] transition-colors disabled:opacity-40 {item.danger
              ? 'text-bad hover:bg-bad/10'
              : 'text-fg-2 hover:bg-hover hover:text-fg'}"
            onclick={() => {
              open = false;
              item.onselect();
            }}
          >
            {#if item.icon}<item.icon size={15} />{/if}
            {item.label}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>
