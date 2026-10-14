<script lang="ts">
  import { ArrowUpRight, CircleCheck, CircleX, Lock, LoaderCircle, Pencil, Plug, Save } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { HostAccess, Settings } from "../../lib/types";
  import { hostName, urlSourceLabel } from "../../lib/format";
  import { toast, toastError } from "../../lib/state.svelte";
  import { hostHref } from "../../lib/router.svelte";
  import Badge from "../../lib/components/Badge.svelte";
  import Modal from "../../lib/components/Modal.svelte";
  import StatusDot from "../../lib/components/StatusDot.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import Connections from "./Connections.svelte";

  let settings = $state<Settings | null>(null);
  let template = $state("");
  let savingTemplate = $state(false);
  let hosts = $state<HostAccess[] | null>(null);

  let editing = $state<HostAccess | null>(null);
  let editOpen = $state(false);
  let override_url = $state("");
  let override_username = $state("");
  let override_password = $state("");
  let tls_skip_verify = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let test = $state<{ ok: boolean; message: string; latency_ms: number | null } | null>(null);

  const pushHosts = $derived(hosts?.filter((h) => h.source === "push") ?? null);

  async function load() {
    try {
      hosts = await api.hostAccess();
    } catch (e) {
      toastError(e, "Failed to load hosts");
    }
  }

  $effect(() => {
    api
      .settings()
      .then((s) => {
        settings = s;
        template = s.action_url_template;
      })
      .catch((e) => toastError(e, "Failed to load settings"));
    load();
  });

  async function saveTemplate(e: SubmitEvent) {
    e.preventDefault();
    savingTemplate = true;
    try {
      settings = await api.updateSettings({ action_url_template: template });
      template = settings.action_url_template;
      toast("ok", template ? "URL template saved" : "URL template cleared");
      load();
    } catch (err) {
      toastError(err, "Could not save the URL template");
    } finally {
      savingTemplate = false;
    }
  }

  function edit(h: HostAccess) {
    editing = h;
    override_url = h.override_url ?? "";
    override_username = h.override_username ?? "";
    override_password = "";
    tls_skip_verify = h.tls_skip_verify;
    test = null;
    editOpen = true;
  }

  async function saveOverride(e: SubmitEvent) {
    e.preventDefault();
    if (!editing) return;
    saving = true;
    try {
      const patch: Record<string, unknown> = { override_url, override_username, tls_skip_verify };
      if (override_password) patch.override_password = override_password;
      await api.updateHost(editing.id, patch);
      toast("ok", `${hostName(editing)} saved`);
      editOpen = false;
      load();
    } catch (err) {
      toastError(err, "Could not save");
    } finally {
      saving = false;
    }
  }

  async function runTest() {
    if (!editing) return;
    testing = true;
    test = null;
    try {
      test = await api.testHost(editing.id);
    } catch (err) {
      toastError(err, "Test failed");
    } finally {
      testing = false;
    }
  }

  const reported = (h: HostAccess) =>
    h.httpd?.port ? `${h.httpd.ssl ? "https" : "http"}://${h.httpd.address || "*"}:${h.httpd.port}` : h.httpd?.unixsocket ? "unix socket only" : "httpd disabled";
  const sourceTone = (h: HostAccess) => (!h.monit_url_source ? "bad" : h.monit_url_source === "remote_addr" ? "warn" : "muted");
</script>

<div class="space-y-8">
  <form class="card space-y-4 p-5 sm:p-6" onsubmit={saveTemplate}>
    <div>
      <div class="flex items-center gap-2">
        <h2 class="card-title">Action URL template</h2>
        {#if settings?.action_url_template_managed}
          <span class="inline-flex items-center gap-1 rounded-md border border-line px-1.5 py-px text-[10px] font-medium whitespace-nowrap text-fg-3" title="Set in the configuration file — edit it there"><Lock size={10} /> managed by config</span>
        {/if}
      </div>
      <p class="mt-1 text-xs text-fg-3">
        How Monarch reaches a pushing agent's Monit HTTP interface for start, stop and restart, unless the host has its own override. Useful when Monit listens on all interfaces and
        the address it pushes from is not reachable, e.g. <code class="num">{"http://{short_hostname}.tailnet.ts.net:{port}"}</code>.
      </p>
    </div>
    <input class="input num" placeholder={"{scheme}://{short_hostname}.tailnet.ts.net:{port}"} bind:value={template} disabled={!settings || settings.action_url_template_managed} />
    <p class="hint">
      Placeholders: <code class="num">{"{hostname}"}</code>, <code class="num">{"{short_hostname}"}</code>, <code class="num">{"{scheme}"}</code>, <code class="num">{"{port}"}</code>,
      <code class="num">{"{address}"}</code> (reported httpd address, else the push source). Leave empty to use the address Monit reports.
    </p>
    {#if settings && !settings.action_url_template_managed}
      <div class="flex justify-end">
        <button class="btn btn-primary btn-sm" disabled={savingTemplate || template === settings.action_url_template}>
          {#if savingTemplate}<LoaderCircle size={14} class="animate-spin" />{:else}<Save size={14} />{/if} Save template
        </button>
      </div>
    {/if}
  </form>

  <div class="space-y-4">
    <div>
      <h2 class="card-title">Pushing hosts</h2>
      <p class="mt-0.5 text-xs text-fg-3">Agents that report to the collector, and the URL Monarch uses for their service actions.</p>
    </div>
    {#if !pushHosts}
      <div class="card space-y-3 p-5">{#each Array(3) as _, i (i)}<div class="skeleton h-12 w-full"></div>{/each}</div>
    {:else if pushHosts.length === 0}
      <div class="card p-5 text-sm text-fg-3">No agent pushes to Monarch yet. See Settings › Collector.</div>
    {:else}
      <div class="card overflow-x-auto">
        <table class="table min-w-[760px]">
          <thead>
            <tr><th class="pl-5">Host</th><th>Reported httpd</th><th>Action URL</th><th>Credentials</th><th class="pr-5"></th></tr>
          </thead>
          <tbody>
            {#each pushHosts as h (h.id)}
              <tr>
                <td class="pl-5">
                  <div class="flex items-center gap-2">
                    <StatusDot tone={h.online ? "ok" : "muted"} size={7} />
                    <span class="font-semibold text-fg">{hostName(h)}</span>
                  </div>
                  {#if h.display_name}<div class="num mt-0.5 text-[11px] text-fg-3">{h.hostname}</div>{/if}
                </td>
                <td class="num text-xs text-fg-2">
                  <div>{reported(h)}</div>
                  {#if h.remote_addr}<div class="mt-0.5 text-[11px] text-fg-3" title="Address the agent pushes from">from {h.remote_addr}</div>{/if}
                </td>
                <td class="max-w-80">
                  <div class="num truncate text-xs text-fg" title={h.monit_url ?? ""}>{h.monit_url ?? "—"}</div>
                  <div class="mt-1">
                    <Badge size="sm" tone={sourceTone(h)}>{h.monit_url_source ? urlSourceLabel[h.monit_url_source] : "no httpd"}</Badge>
                  </div>
                </td>
                <td class="text-xs text-fg-2">
                  {h.override_username ? "override" : h.has_reported_credentials ? "registered by Monit" : "none"}
                  {#if h.tls_skip_verify}<div class="text-[11px] text-fg-3">TLS not verified</div>{/if}
                </td>
                <td class="pr-5">
                  <div class="flex items-center justify-end gap-1">
                    <a class="btn btn-ghost btn-sm btn-icon" href={hostHref(h.id)} title="Open host" aria-label="Open host {hostName(h)}"><ArrowUpRight size={14} /></a>
                    <button class="btn btn-ghost btn-sm btn-icon" onclick={() => edit(h)} title="Edit override" aria-label="Edit override for {hostName(h)}"><Pencil size={14} /></button>
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>

  <Connections />
</div>

<Modal bind:open={editOpen} title="Monit HTTP interface · {editing ? hostName(editing) : ''}" width="max-w-xl">
  {#if editing}
    <form class="space-y-4" onsubmit={saveOverride}>
      <div>
        <label class="label" for="ha-url">URL override</label>
        <input id="ha-url" class="input num" placeholder={editing.monit_url ?? "https://host.example.com:2812"} bind:value={override_url} />
        <p class="hint">Leave empty to use {settings?.action_url_template ? "the URL template" : "the reported address"}.</p>
      </div>
      <div class="grid gap-4 sm:grid-cols-2">
        <div>
          <label class="label" for="ha-user">Username override</label>
          <input id="ha-user" class="input" autocomplete="off" bind:value={override_username} />
        </div>
        <div>
          <label class="label" for="ha-pass">Password override</label>
          <input id="ha-pass" class="input" type="password" autocomplete="new-password" placeholder={editing.has_override_password ? "•••••••• (unchanged)" : ""} bind:value={override_password} />
        </div>
      </div>
      <div class="flex items-center justify-between gap-4 rounded-xl border border-line px-4 py-3">
        <div>
          <div class="text-[13px] font-medium text-fg">Skip TLS verification</div>
          <div class="text-xs text-fg-3">Monit's httpd commonly uses a self-signed certificate.</div>
        </div>
        <Switch bind:checked={tls_skip_verify} label="Skip TLS verification" />
      </div>
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex flex-wrap items-center gap-3">
          <button type="button" class="btn btn-sm" onclick={runTest} disabled={testing} title="Tests the saved settings">
            {#if testing}<LoaderCircle size={14} class="animate-spin" />{:else}<Plug size={14} />{/if} Test saved
          </button>
          {#if test}
            <span class="tone-{test.ok ? 'ok' : 'bad'} flex items-center gap-1.5 text-xs text-tone animate-in">
              {#if test.ok}<CircleCheck size={14} />{:else}<CircleX size={14} />{/if}
              {test.message}{test.latency_ms !== null ? ` (${test.latency_ms.toFixed(0)} ms)` : ""}
            </span>
          {/if}
        </div>
        <div class="flex gap-2">
          <button type="button" class="btn btn-sm" onclick={() => (editOpen = false)}>Cancel</button>
          <button class="btn btn-primary btn-sm" disabled={saving}>{#if saving}<LoaderCircle size={14} class="animate-spin" />{:else}<Save size={14} />{/if} Save</button>
        </div>
      </div>
    </form>
  {/if}
</Modal>
