<script lang="ts">
  import { CircleCheck, CircleX, Info, TriangleAlert, X } from "@lucide/svelte";
  import { dismissToast, toasts } from "../state.svelte";
  import { router } from "../router.svelte";
  const icons = { ok: CircleCheck, bad: CircleX, warn: TriangleAlert, info: Info };
</script>

<div class="pointer-events-none fixed right-4 bottom-4 z-[70] flex w-[min(380px,calc(100vw-2rem))] flex-col gap-2" aria-live="polite">
  {#each toasts as t (t.id)}
    {@const Icon = icons[t.tone]}
    <div class="tone-{t.tone} pointer-events-auto flex items-start gap-3 rounded-xl border border-line-strong bg-surface-solid/95 p-3.5 shadow-2xl backdrop-blur animate-in">
      <div class="mt-0.5 text-tone"><Icon size={17} /></div>
      <button
        class="min-w-0 flex-1 text-left {t.href ? 'cursor-pointer' : 'cursor-default'}"
        onclick={() => {
          if (t.href) {
            router.go(t.href);
            dismissToast(t.id);
          }
        }}
      >
        <div class="truncate text-[13px] font-semibold text-fg">{t.title}</div>
        {#if t.body}<div class="mt-0.5 line-clamp-2 text-xs text-fg-2">{t.body}</div>{/if}
      </button>
      <button class="text-fg-3 hover:text-fg" aria-label="Dismiss" onclick={() => dismissToast(t.id)}><X size={14} /></button>
    </div>
  {/each}
</div>
