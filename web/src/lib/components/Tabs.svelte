<script lang="ts">
  let {
    tabs,
    active,
    href,
  }: { tabs: { id: string; label: string; count?: number | null }[]; active: string; href: (id: string) => string } = $props();
</script>

<nav class="-mb-px flex gap-1 overflow-x-auto border-b border-line" aria-label="Tabs">
  {#each tabs as t (t.id)}
    <a
      href={href(t.id)}
      class="relative flex items-center gap-2 px-3.5 py-2.5 text-[13px] font-medium whitespace-nowrap transition-colors {active === t.id
        ? 'text-fg'
        : 'text-fg-3 hover:text-fg-2'}"
      aria-current={active === t.id ? "page" : undefined}
    >
      {t.label}
      {#if t.count !== undefined && t.count !== null}
        <span class="num rounded-md bg-surface-2 px-1.5 py-px text-[10px] text-fg-3">{t.count}</span>
      {/if}
      {#if active === t.id}
        <span class="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-accent-gradient"></span>
      {/if}
    </a>
  {/each}
</nav>
