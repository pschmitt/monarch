<script lang="ts">
  import { BellOff, CircleCheck, CircleX, LoaderCircle, Plug, Save, Trash } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { HostDetail } from "../../lib/types";
  import { hostName, toLocalInput } from "../../lib/format";
  import { confirm, toast, toastError } from "../../lib/state.svelte";
  import { router } from "../../lib/router.svelte";
  import Switch from "../../lib/components/Switch.svelte";

  let { host, onsaved }: { host: HostDetail; onsaved: (h: HostDetail) => void } = $props();

  // svelte-ignore state_referenced_locally
  const initial = host;
  let display_name = $state(initial.display_name ?? "");
  let description = $state(initial.description ?? "");
  let override_url = $state(initial.override_url ?? "");
  let override_username = $state(initial.override_username ?? "");
  let override_password = $state("");
  let tls_skip_verify = $state(initial.tls_skip_verify);
  let muted_until = $state(toLocalInput(initial.muted_until));
  let saving = $state(false);
  let testing = $state(false);
  let test = $state<{ ok: boolean; message: string; latency_ms: number | null } | null>(null);

  async function save(e?: SubmitEvent) {
    e?.preventDefault();
    saving = true;
    try {
      const patch: Record<string, unknown> = {
        display_name,
        description,
        override_url,
        override_username,
        tls_skip_verify,
        muted_until: muted_until ? Math.floor(new Date(muted_until).getTime() / 1000) : null,
      };
      if (override_password) patch.override_password = override_password;
      const h = await api.updateHost(host.id, patch);
      override_password = "";
      onsaved(h);
      toast("ok", "Host settings saved");
    } catch (err) {
      toastError(err, "Could not save");
    } finally {
      saving = false;
    }
  }

  async function mute(hours: number) {
    muted_until = toLocalInput(Date.now() / 1000 + hours * 3600);
    await save();
  }

  async function runTest() {
    testing = true;
    test = null;
    try {
      test = await api.testHost(host.id);
    } catch (err) {
      test = { ok: false, message: err instanceof Error ? err.message : String(err), latency_ms: null };
    } finally {
      testing = false;
    }
  }

  async function remove() {
    const ok = await confirm({
      title: `Remove ${hostName(host)}?`,
      body: "All services, metrics and events for this host are deleted. If Monit keeps reporting, the host will re-appear as new.",
      confirm: "Remove host",
      danger: true,
    });
    if (!ok) return;
    try {
      await api.deleteHost(host.id);
      toast("ok", `${hostName(host)} removed`);
      router.go("/hosts");
    } catch (err) {
      toastError(err, "Could not remove host");
    }
  }
</script>

<form class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_340px]" onsubmit={save}>
  <div class="space-y-6">
    <section class="card space-y-4 p-5 sm:p-6">
      <div>
        <h3 class="card-title">General</h3>
        <p class="mt-1 text-xs text-fg-3">How this host appears in Monarch.</p>
      </div>
      <div class="grid gap-4 sm:grid-cols-2">
        <div>
          <label class="label" for="dn">Display name</label>
          <input id="dn" class="input" placeholder={host.hostname} bind:value={display_name} />
        </div>
        <div>
          <label class="label" for="hn">Reported hostname</label>
          <input id="hn" class="input num opacity-70" value={host.hostname} disabled />
        </div>
      </div>
      <div>
        <label class="label" for="desc">Description</label>
        <textarea id="desc" class="input" rows="2" placeholder="What runs here, who owns it…" bind:value={description}></textarea>
      </div>
    </section>

    <section class="card space-y-4 p-5 sm:p-6">
      <div>
        <h3 class="card-title">Monit HTTP interface</h3>
        <p class="mt-1 text-xs text-fg-3">
          Used to start, stop and restart services. By default Monarch uses the address and credentials Monit registers (<code class="num">set httpd</code> +
          <code class="num">register credentials</code>).
        </p>
      </div>
      <div class="grid gap-3 rounded-xl border border-line bg-surface-2/50 p-3 text-xs sm:grid-cols-3">
        <div><div class="eyebrow">Reported httpd</div><div class="num mt-1 text-fg-2">{host.httpd ? `${host.httpd.ssl ? "https" : "http"}://${host.httpd.address ?? "?"}:${host.httpd.port ?? "?"}` : "not enabled"}</div></div>
        <div><div class="eyebrow">Credentials</div><div class="mt-1 text-fg-2">{host.has_reported_credentials ? "registered by Monit" : "not registered"}</div></div>
        <div><div class="eyebrow">Effective URL</div><div class="num mt-1 truncate text-fg-2">{host.monit_url ?? "—"}</div></div>
      </div>
      <div>
        <label class="label" for="url">URL override</label>
        <input id="url" class="input num" placeholder="https://host.example.com:2812" bind:value={override_url} />
        <p class="hint">Leave empty to use the reported address.</p>
      </div>
      <div class="grid gap-4 sm:grid-cols-2">
        <div>
          <label class="label" for="ou">Username override</label>
          <input id="ou" class="input" autocomplete="off" bind:value={override_username} />
        </div>
        <div>
          <label class="label" for="op">Password override</label>
          <input id="op" class="input" type="password" autocomplete="new-password" placeholder={host.has_override_password ? "•••••••• (unchanged)" : ""} bind:value={override_password} />
        </div>
      </div>
      <div class="flex items-center justify-between gap-4 rounded-xl border border-line px-4 py-3">
        <div>
          <div class="text-[13px] font-medium text-fg">Skip TLS verification</div>
          <div class="text-xs text-fg-3">Monit's httpd commonly uses a self-signed certificate.</div>
        </div>
        <Switch bind:checked={tls_skip_verify} label="Skip TLS verification" />
      </div>
      <div class="flex flex-wrap items-center gap-3">
        <button type="button" class="btn" onclick={runTest} disabled={testing}>
          {#if testing}<LoaderCircle size={15} class="animate-spin" />{:else}<Plug size={15} />{/if} Test connection
        </button>
        {#if test}
          <span class="tone-{test.ok ? 'ok' : 'bad'} flex items-center gap-1.5 text-xs text-tone animate-in">
            {#if test.ok}<CircleCheck size={14} />{:else}<CircleX size={14} />{/if}
            {test.message}{test.latency_ms !== null ? ` (${test.latency_ms.toFixed(0)} ms)` : ""}
          </span>
        {/if}
      </div>
    </section>

    <div class="flex justify-end">
      <button class="btn btn-primary" disabled={saving}>{#if saving}<LoaderCircle size={15} class="animate-spin" />{:else}<Save size={15} />{/if} Save changes</button>
    </div>
  </div>

  <aside class="space-y-6">
    <section class="card space-y-4 p-5">
      <div class="flex items-center gap-2">
        <BellOff size={15} class="text-fg-3" />
        <h3 class="card-title">Mute notifications</h3>
      </div>
      <p class="text-xs text-fg-3">Events are still recorded, but no notifications are sent until the given time.</p>
      <div class="flex flex-wrap gap-2">
        {#each [[1, "1 hour"], [8, "8 hours"], [24, "1 day"], [168, "1 week"]] as [h, label] (h)}
          <button type="button" class="btn btn-sm" onclick={() => mute(h as number)}>{label}</button>
        {/each}
      </div>
      <div>
        <label class="label" for="mu">Muted until</label>
        <input id="mu" type="datetime-local" class="input num" bind:value={muted_until} />
      </div>
      {#if muted_until}
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          onclick={() => {
            muted_until = "";
            save();
          }}>Unmute now</button
        >
      {/if}
    </section>

    <section class="card tone-bad space-y-3 border-tone-soft p-5">
      <h3 class="card-title text-bad">Danger zone</h3>
      <p class="text-xs text-fg-3">Remove the host together with its history. Useful for decommissioned machines.</p>
      <button type="button" class="btn btn-danger" onclick={remove}><Trash size={15} /> Remove host</button>
    </section>
  </aside>
</form>
