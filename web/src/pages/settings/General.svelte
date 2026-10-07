<script lang="ts">
  import { LoaderCircle, Save } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { Settings } from "../../lib/types";
  import { toast, toastError } from "../../lib/state.svelte";

  let s = $state<Settings | null>(null);
  let saving = $state(false);

  $effect(() => {
    api.settings().then((v) => (s = v)).catch((e) => toastError(e, "Failed to load settings"));
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!s) return;
    saving = true;
    try {
      s = await api.updateSettings({ public_url: s.public_url, retention: s.retention, heartbeat_grace: s.heartbeat_grace });
      toast("ok", "Settings saved");
    } catch (err) {
      toastError(err, "Could not save settings");
    } finally {
      saving = false;
    }
  }
</script>

{#if !s}
  <div class="card space-y-4 p-6">{#each Array(4) as _, i (i)}<div class="skeleton h-10 w-full"></div>{/each}</div>
{:else}
  <form class="space-y-6" onsubmit={save}>
    <section class="card space-y-4 p-5 sm:p-6">
      <div>
        <h2 class="card-title">Public URL</h2>
        <p class="mt-1 text-xs text-fg-3">Where users and Monit agents reach Monarch. Used for the collector snippet and links in notifications.</p>
      </div>
      <input class="input num" type="url" placeholder="https://monarch.example.com" bind:value={s.public_url} required />
    </section>

    <section class="card space-y-4 p-5 sm:p-6">
      <div>
        <h2 class="card-title">Heartbeat</h2>
        <p class="mt-1 text-xs text-fg-3">A host is marked offline when it misses this many consecutive poll cycles.</p>
      </div>
      <div class="flex items-center gap-4">
        <input type="range" min="2" max="10" step="1" class="flex-1 accent-[var(--accent)]" bind:value={s.heartbeat_grace} aria-label="Heartbeat grace" />
        <span class="num w-24 rounded-lg border border-line bg-surface-2 px-2.5 py-1 text-center text-sm text-fg">{s.heartbeat_grace} cycles</span>
      </div>
    </section>

    <section class="card space-y-4 p-5 sm:p-6">
      <div>
        <h2 class="card-title">Data retention</h2>
        <p class="mt-1 text-xs text-fg-3">Raw samples are rolled up into 5-minute and hourly buckets. Longer retention means a bigger database.</p>
      </div>
      <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {#each [["raw_hours", "Raw samples", "hours"], ["rollup_5m_days", "5-minute rollups", "days"], ["rollup_1h_days", "Hourly rollups", "days"], ["events_days", "Events", "days"]] as [key, label, unit] (key)}
          <div>
            <label class="label" for={key}>{label}</label>
            <div class="relative">
              <input id={key} class="input num pr-14" type="number" min="1" bind:value={s.retention[key as keyof Settings["retention"]]} />
              <span class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-fg-3">{unit}</span>
            </div>
          </div>
        {/each}
      </div>
    </section>

    <div class="flex justify-end">
      <button class="btn btn-primary" disabled={saving}>{#if saving}<LoaderCircle size={15} class="animate-spin" />{:else}<Save size={15} />{/if} Save settings</button>
    </div>
  </form>
{/if}
