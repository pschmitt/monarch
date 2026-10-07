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

  function onwin(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onclick={onwin} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="relative inline-block" bind:this={root}>
  <button class="btn btn-ghost btn-sm btn-icon" aria-haspopup="menu" aria-expanded={open} aria-label={label} onclick={() => (open = !open)}>
    {@render trigger()}
  </button>
  {#if open}
    <div
      role="menu"
      class="absolute z-40 mt-1.5 min-w-44 overflow-hidden rounded-xl border border-line-strong bg-surface-solid p-1 shadow-2xl animate-in {align === 'right' ? 'right-0' : 'left-0'}"
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
