<script lang="ts">
  import { Check, CircleQuestionMark, Eye, EyeOff, Play, RefreshCcw, RotateCw, Square, Trash, TriangleAlert, X } from "@lucide/svelte";
  import { confirmState } from "../state.svelte";

  function done(ok: boolean) {
    const r = confirmState.req;
    confirmState.req = null;
    r?.resolve(ok);
  }
  function onkey(e: KeyboardEvent) {
    if (!confirmState.req) return;
    if (e.key === "Escape") done(false);
    if (e.key === "Enter") done(true);
  }
  function actionIcon(action?: string) {
    switch (action) {
      case "start": return Play;
      case "stop": return Square;
      case "restart": return RotateCw;
      case "monitor": return Eye;
      case "unmonitor": return EyeOff;
      case "remonitor": return RefreshCcw;
      case "delete": case "delete user": case "revoke": return Trash;
      case "reset": return RefreshCcw;
      default: return Check;
    }
  }
</script>

<svelte:window onkeydown={onkey} />

{#if confirmState.req}
  {@const req = confirmState.req}
  {@const ActionIcon = actionIcon(req.confirmIcon ?? req.confirm?.toLowerCase())}
  <div class="fixed inset-0 z-[60] flex items-center justify-center p-4">
    <button class="absolute inset-0 cursor-default bg-[var(--mn-scrim)] backdrop-blur-sm" aria-label="Cancel" onclick={() => done(false)}></button>
    <div role="alertdialog" aria-modal="true" class="relative w-full max-w-sm rounded-2xl border border-line-strong bg-surface-solid p-5 shadow-2xl animate-in">
      <div class="flex gap-3.5">
        <div class="tone-{req.danger ? 'bad' : 'info'} flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-tone-soft text-tone">
          {#if req.danger}<TriangleAlert size={18} />{:else}<CircleQuestionMark size={18} />{/if}
        </div>
        <div class="min-w-0">
          <h2 class="text-[15px] font-semibold text-fg">{req.title}</h2>
          {#if req.body}<p class="mt-1 text-[13px] leading-relaxed text-fg-2">{req.body}</p>{/if}
        </div>
      </div>
      <div class="mt-5 flex justify-end gap-2">
        <button class="btn btn-ghost" onclick={() => done(false)}><X size={15} aria-hidden="true" /> Cancel</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn {req.danger ? 'btn-danger' : 'btn-primary'}" autofocus onclick={() => done(true)}><ActionIcon size={15} aria-hidden="true" /> {req.confirm ?? "Confirm"}</button>
      </div>
    </div>
  </div>
{/if}
