<script lang="ts">
  import { BellOff, Pencil, RotateCcw } from "@lucide/svelte";
  import { api } from "../api";
  import type { Channel, CheckAlert } from "../types";
  import { ago } from "../format";
  import { serviceHref } from "../router.svelte";
  import { clock, confirm, toast, toastError } from "../state.svelte";
  import CheckAlerts from "./CheckAlerts.svelte";

  let { channels }: { channels: Channel[] | null } = $props();

  let rows = $state<CheckAlert[] | null>(null);
  let editing = $state<CheckAlert | null>(null);
  let open = $state(false);

  async function load() {
    try {
      rows = await api.checkAlerts();
    } catch (e) {
      toastError(e, "Failed to load check alert settings");
    }
  }
  $effect(() => {
    load();
  });

  const channelNames = (ids: number[]) => ids.map((id) => channels?.find((c) => c.id === id)?.name ?? `#${id}`).join(", ");

  async function reset(r: CheckAlert) {
    if (!(await confirm({ title: `Reset ${r.service}?`, body: `${r.service} on ${r.host} goes back to the generic notification settings.`, confirm: "Reset" }))) return;
    try {
      await api.resetCheckAlert(r.host_id, r.service);
      toast("ok", "Back to the generic settings");
      load();
    } catch (e) {
      toastError(e, "Could not reset");
    }
  }
</script>

<section class="card overflow-hidden">
  <div class="border-b border-line px-5 py-4">
    <h2 class="card-title">Check-specific alerts</h2>
    <p class="mt-0.5 text-xs text-fg-3">Checks with their own notification settings. Add one from a check's page with the “Alerts” button.</p>
  </div>
  {#if !rows}
    <div class="skeleton m-5 h-16"></div>
  {:else if rows.length === 0}
    <p class="px-5 py-6 text-center text-xs text-fg-3">No check has its own settings, everything follows the generic ones.</p>
  {:else}
    <div class="overflow-x-auto">
      <table class="table min-w-[560px]">
        <thead><tr><th class="pl-5">Check</th><th>Overrides</th><th>Changed</th><th class="pr-5"></th></tr></thead>
        <tbody>
          {#each rows as r (`${r.host_id}/${r.service}`)}
            <tr>
              <td class="pl-5">
                <a class="font-medium text-fg hover:underline" href={serviceHref(r.host_id, r.service)}>{r.service}</a>
                <div class="text-[11px] text-fg-3">{r.host}</div>
              </td>
              <td>
                <div class="flex flex-wrap gap-1.5 text-[11px]">
                  {#if r.muted}<span class="inline-flex items-center gap-1 rounded-md border border-line px-1.5 py-0.5 text-warn"><BellOff size={11} /> muted</span>{/if}
                  {#if !r.muted && r.events}<span class="rounded-md border border-line px-1.5 py-0.5 text-fg-2" title={r.events.join(", ")}>{r.events.length} event kind{r.events.length === 1 ? "" : "s"}</span>{/if}
                  {#if !r.muted && r.channels}<span class="rounded-md border border-line px-1.5 py-0.5 text-fg-2" title={channelNames(r.channels)}>→ {r.channels.length ? channelNames(r.channels) : "no channel"}</span>{/if}
                </div>
              </td>
              <td class="num text-xs text-fg-3">{r.updated_at ? ago(r.updated_at, clock.now) : ""}</td>
              <td class="pr-5 text-right">
                <button class="btn btn-ghost btn-sm btn-icon" aria-label="Edit {r.service}" onclick={() => { editing = r; open = true; }}><Pencil size={13} /></button>
                <button class="btn btn-ghost btn-sm btn-icon hover:text-bad" aria-label="Reset {r.service}" onclick={() => reset(r)}><RotateCcw size={13} /></button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

{#if editing}
  <CheckAlerts bind:open hostId={editing.host_id} service={editing.service} hostName={editing.host ?? ""} onsaved={load} />
{/if}
