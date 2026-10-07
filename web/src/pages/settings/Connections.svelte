<script lang="ts">
  import { ArrowDownToLine, ArrowUpRight, Ellipsis, LoaderCircle, Lock, Pencil, Plus, RefreshCw, Terminal, Trash } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { Target } from "../../lib/types";
  import { ago, datetime } from "../../lib/format";
  import { clock, confirm, fleet, onLiveTarget, toast, toastError, ui } from "../../lib/state.svelte";
  import { hostHref } from "../../lib/router.svelte";
  import Empty from "../../lib/components/Empty.svelte";
  import Menu from "../../lib/components/Menu.svelte";
  import Modal from "../../lib/components/Modal.svelte";
  import StatusDot from "../../lib/components/StatusDot.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import TargetForm from "../../lib/components/TargetForm.svelte";

  let targets = $state<Target[] | null>(null);
  let editing = $state<Target | null>(null);
  let editOpen = $state(false);
  let polling = $state<number | null>(null);

  async function load() {
    try {
      targets = await api.targets();
    } catch (e) {
      toastError(e, "Failed to load connections");
    }
  }
  $effect(() => {
    const t = setInterval(load, 15_000);
    return () => clearInterval(t);
  });
  // Initial load, and refresh after the add-host modal closes.
  $effect(() => {
    if (!ui.addHost) load();
  });

  function replace(t: Target) {
    if (targets) targets = targets.map((x) => (x.id === t.id ? t : x));
  }

  // Live status after each poll; unknown ids mean a connection was added elsewhere.
  $effect(() =>
    onLiveTarget((t) => {
      if (!targets) return;
      if (targets.some((x) => x.id === t.id)) replace(t);
      else targets = [...targets, t];
    }),
  );

  async function pollNow(t: Target) {
    polling = t.id;
    try {
      const r = await api.pollTarget(t.id);
      replace(r);
      toast(r.last_status === "ok" ? "ok" : "bad", r.last_status === "ok" ? `Polled ${t.name}` : `Polling ${t.name} failed`, r.last_status === "ok" ? undefined : (r.last_status ?? undefined));
    } catch (e) {
      toastError(e, `Could not poll ${t.name}`);
    } finally {
      polling = null;
    }
  }

  async function setEnabled(t: Target, enabled: boolean) {
    try {
      replace(await api.updateTarget(t.id, { enabled }));
    } catch (e) {
      toastError(e, "Could not update connection");
      load();
    }
  }

  async function remove(t: Target) {
    const ok = await confirm({
      title: `Delete connection ${t.name}?`,
      body: "Monarch stops polling this agent. The host and its history stay — remove the host separately if you no longer need it.",
      confirm: "Delete connection",
      danger: true,
    });
    if (!ok) return;
    try {
      await api.deleteTarget(t.id);
      toast("ok", `Connection ${t.name} deleted`);
      load();
    } catch (e) {
      toastError(e, "Could not delete connection");
    }
  }

  function edit(t: Target) {
    editing = t;
    editOpen = true;
  }

  const statusTone = (t: Target) => (!t.enabled ? "muted" : t.last_status === "ok" ? "ok" : t.last_status ? "bad" : "warn");
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h2 class="card-title">Connections</h2>
      <p class="mt-0.5 text-xs text-fg-3">Monit agents Monarch polls itself — directly or through an SSH tunnel.</p>
    </div>
    <button class="btn btn-primary btn-sm" onclick={() => (ui.addHost = true)}><Plus size={14} /> Add connection</button>
  </div>

  {#if !targets}
    <div class="card space-y-3 p-5">{#each Array(3) as _, i (i)}<div class="skeleton h-12 w-full"></div>{/each}</div>
  {:else if targets.length === 0}
    <div class="card">
      <Empty icon={ArrowDownToLine} title="No pull connections" body="Let Monarch poll Monit's HTTP interface — handy for hosts that can't reach Monarch, or with SSH for agents that only listen on localhost.">
        <button class="btn btn-primary" onclick={() => (ui.addHost = true)}><Plus size={14} /> Add connection</button>
      </Empty>
    </div>
  {:else}
    <div class="card overflow-x-auto">
      <table class="table min-w-[760px]">
        <thead>
          <tr><th class="pl-5">Connection</th><th>Via</th><th>Status</th><th>Last poll</th><th>On</th><th class="pr-5"></th></tr>
        </thead>
        <tbody>
          {#each targets as t (t.id)}
            {@const host = t.host_id ? fleet.hosts[t.host_id] : null}
            <tr class={t.enabled ? "" : "opacity-60"}>
              <td class="pl-5">
                <div class="flex items-center gap-2">
                  <span class="font-semibold text-fg">{t.name}</span>
                  {#if t.managed}
                    <span class="inline-flex items-center gap-1 rounded-md border border-line px-1.5 py-px text-[10px] font-medium whitespace-nowrap text-fg-3" title="Declared in the configuration file — edit it there"><Lock size={10} /> managed by config</span>
                  {/if}
                </div>
                <div class="num mt-0.5 max-w-64 truncate text-[11px] text-fg-3" title={t.url}>{t.url}</div>
              </td>
              <td>
                {#if t.ssh}
                  <span class="num inline-flex max-w-56 items-center gap-1.5 truncate rounded-md border border-[color-mix(in_oklab,var(--accent)_35%,transparent)] bg-[color-mix(in_oklab,var(--accent)_10%,transparent)] px-2 py-0.5 text-[11px] text-fg" title="ssh -W via {t.ssh.destination}{t.ssh.port ? ` -p ${t.ssh.port}` : ''}">
                    <Terminal size={11} class="shrink-0 text-accent" />{t.ssh.destination}{t.ssh.port && t.ssh.port !== 22 ? `:${t.ssh.port}` : ""}
                  </span>
                {:else}
                  <span class="text-xs text-fg-3">direct</span>
                {/if}
              </td>
              <td class="max-w-56">
                <div class="flex items-center gap-2 text-xs" title={t.last_status ?? "not polled yet"}>
                  <StatusDot tone={statusTone(t)} pulse={t.enabled && !!t.last_status && t.last_status !== "ok"} size={7} />
                  <span class="truncate {t.last_status && t.last_status !== 'ok' ? 'text-bad' : 'text-fg-2'}">
                    {!t.enabled ? "paused" : t.last_status === "ok" ? "OK" : (t.last_status ?? "pending")}
                  </span>
                </div>
              </td>
              <td class="num text-xs whitespace-nowrap" title={datetime(t.last_polled_at)}>
                <div class="text-fg-2">{t.last_polled_at ? ago(t.last_polled_at, clock.now) : "never"}</div>
                <div class="text-[11px] text-fg-3">every {t.interval}s</div>
              </td>
              <td><Switch checked={t.enabled} label="Enabled" disabled={t.managed} onchange={(v) => setEnabled(t, v)} /></td>
              <td class="pr-5">
                <div class="flex items-center justify-end gap-1">
                  {#if t.host_id}
                    <a class="btn btn-ghost btn-sm btn-icon" href={hostHref(t.host_id)} title="Open host {host ? (host.display_name ?? host.hostname) : ''}" aria-label="Open host"><ArrowUpRight size={14} /></a>
                  {/if}
                  <button class="btn btn-ghost btn-sm btn-icon" onclick={() => pollNow(t)} disabled={polling === t.id || !t.enabled} aria-label="Poll {t.name} now" title="Poll now">
                    {#if polling === t.id}<LoaderCircle size={14} class="animate-spin" />{:else}<RefreshCw size={14} />{/if}
                  </button>
                  <Menu
                    label="Actions for {t.name}"
                    items={[
                      { label: "Poll now", icon: RefreshCw, disabled: !t.enabled, onselect: () => pollNow(t) },
                      { label: t.managed ? "Edit (managed by config)" : "Edit", icon: Pencil, disabled: t.managed, onselect: () => edit(t) },
                      "sep",
                      { label: "Delete", icon: Trash, danger: true, disabled: t.managed, onselect: () => remove(t) },
                    ]}
                  >
                    {#snippet trigger()}<Ellipsis size={16} />{/snippet}
                  </Menu>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<Modal bind:open={editOpen} title="Edit {editing?.name ?? 'connection'}" width="max-w-2xl">
  {#if editing}
    {#key editing.id}
      <TargetForm
        target={editing}
        submitLabel="Save"
        oncancel={() => (editOpen = false)}
        onsaved={(t) => {
          replace(t);
          editOpen = false;
          toast("ok", `Connection ${t.name} saved`);
        }}
      />
    {/key}
  {/if}
</Modal>
