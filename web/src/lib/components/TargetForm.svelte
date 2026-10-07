<script lang="ts">
  import { CircleCheck, CircleX, LoaderCircle, Plug, Terminal } from "@lucide/svelte";
  import { api } from "../api";
  import type { Target, TargetInput, TargetTestResult } from "../types";
  import { toastError } from "../state.svelte";
  import Switch from "./Switch.svelte";

  let {
    target = null,
    submitLabel = "Add",
    onsaved,
    oncancel,
  }: {
    target?: Target | null;
    submitLabel?: string;
    onsaved: (t: Target) => void;
    oncancel?: () => void;
  } = $props();

  const MASK = "********";
  // svelte-ignore state_referenced_locally
  const t0 = target;
  let name = $state(t0?.name ?? "");
  let url = $state(t0?.url ?? "");
  let username = $state(t0?.username ?? "");
  let password = $state("");
  let useSsh = $state(!!t0?.ssh);
  let destination = $state(t0?.ssh?.destination ?? "");
  let port = $state<number | null>(t0?.ssh?.port ?? null);
  let interval = $state(t0?.interval ?? 30);
  let tls = $state(t0?.tls_skip_verify ?? true);
  let enabled = $state(t0?.enabled ?? true);

  let testing = $state(false);
  let saving = $state(false);
  let result = $state<TargetTestResult | null>(null);

  function input(): TargetInput {
    return {
      name: name.trim(),
      url: url.trim(),
      username: username.trim() || null,
      // Unchanged secret: send the mask so the backend keeps the stored value.
      password: password ? password : t0?.has_password ? MASK : null,
      ssh: useSsh && destination.trim() ? { destination: destination.trim(), port: port || null } : null,
      interval: Math.max(5, Number(interval) || 30),
      tls_skip_verify: tls,
      enabled,
    };
  }

  async function test() {
    testing = true;
    result = null;
    try {
      // Stored target with an unchanged password: the backend only has the
      // secret, so test the saved connection by id.
      result = await api.testTarget(t0 && !password && t0.has_password ? { id: t0.id } : input());
    } catch (e) {
      result = { ok: false, message: e instanceof Error ? e.message : String(e), hostname: null, monit_version: null, services: null, latency_ms: null };
    } finally {
      testing = false;
    }
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    try {
      const body = input();
      const t = t0 ? await api.updateTarget(t0.id, body) : await api.createTarget(body);
      onsaved(t);
    } catch (err) {
      toastError(err, t0 ? "Could not save connection" : "Could not add connection");
    } finally {
      saving = false;
    }
  }

  const canSubmit = $derived(!!name.trim() && !!url.trim() && (!useSsh || !!destination.trim()));
</script>

<form class="space-y-4" onsubmit={submit}>
  <div class="grid gap-4 sm:grid-cols-2">
    <div>
      <label class="label" for="t-name">Name</label>
      <input id="t-name" class="input" placeholder="rofl-10" bind:value={name} required autocomplete="off" />
    </div>
    <div>
      <label class="label" for="t-interval">Poll interval</label>
      <div class="relative">
        <input id="t-interval" class="input num pr-16" type="number" min="5" bind:value={interval} />
        <span class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-xs text-fg-3">seconds</span>
      </div>
    </div>
  </div>

  <div>
    <label class="label" for="t-url">Monit URL</label>
    <input id="t-url" class="input num" placeholder="http://127.0.0.1:2812" bind:value={url} required autocomplete="off" />
    <p class="hint">
      {#if useSsh}As seen from the SSH host — Monit's httpd may listen on localhost only.{:else}Must be reachable from the Monarch server.{/if}
    </p>
  </div>

  <div class="grid gap-4 sm:grid-cols-2">
    <div>
      <label class="label" for="t-user">Username</label>
      <input id="t-user" class="input" placeholder="admin" bind:value={username} autocomplete="off" />
    </div>
    <div>
      <label class="label" for="t-pass">Password</label>
      <input id="t-pass" class="input" type="password" autocomplete="new-password" placeholder={t0?.has_password ? "•••••••• (unchanged)" : ""} bind:value={password} />
    </div>
  </div>

  <div class="rounded-xl border border-line {useSsh ? 'bg-surface-2/40' : ''} transition-colors">
    <label class="flex cursor-pointer items-center justify-between gap-4 px-4 py-3">
      <span class="flex items-center gap-3">
        <span class="flex h-8 w-8 items-center justify-center rounded-lg border border-line-strong bg-surface-2 text-accent"><Terminal size={15} /></span>
        <span>
          <span class="block text-[13px] font-medium text-fg">Connect via SSH</span>
          <span class="block text-xs text-fg-3">Tunnel through <code class="num">ssh -W</code> using Monarch's SSH key</span>
        </span>
      </span>
      <Switch bind:checked={useSsh} label="Connect via SSH" />
    </label>
    {#if useSsh}
      <div class="grid gap-3 border-t border-line px-4 py-3 sm:grid-cols-[minmax(0,1fr)_120px] animate-in">
        <div>
          <label class="label" for="t-dest">Destination</label>
          <input id="t-dest" class="input num" placeholder="root@host.example.com" bind:value={destination} autocomplete="off" />
        </div>
        <div>
          <label class="label" for="t-port">Port</label>
          <input id="t-port" class="input num" type="number" min="1" max="65535" placeholder="22" bind:value={port} />
        </div>
      </div>
    {/if}
  </div>

  <div class="flex flex-wrap gap-x-6 gap-y-3">
    <label class="flex items-center gap-2.5 text-[13px] text-fg-2"><Switch bind:checked={tls} label="Skip TLS verification" /> Skip TLS verification</label>
    <label class="flex items-center gap-2.5 text-[13px] text-fg-2"><Switch bind:checked={enabled} label="Enabled" /> Enabled</label>
  </div>

  {#if result}
    <div class="tone-{result.ok ? 'ok' : 'bad'} rounded-xl border border-tone-soft bg-tone-soft px-4 py-3 animate-in">
      <div class="flex items-start gap-3">
        <span class="mt-0.5 text-tone">{#if result.ok}<CircleCheck size={17} />{:else}<CircleX size={17} />{/if}</span>
        <div class="min-w-0 flex-1">
          {#if result.ok}
            <div class="text-[13px] font-semibold text-fg">Connected to {result.hostname ?? "Monit"}</div>
            <div class="num mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-xs text-fg-2">
              {#if result.monit_version}<span>Monit {result.monit_version}</span>{/if}
              {#if result.services !== null}<span>{result.services} services</span>{/if}
              {#if result.latency_ms !== null}<span>{result.latency_ms.toFixed(0)} ms</span>{/if}
            </div>
          {:else}
            <div class="text-[13px] font-semibold text-tone">Connection failed</div>
            <div class="num mt-1 text-xs break-words text-fg-2">{result.message}</div>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <div class="flex flex-wrap items-center gap-2 border-t border-line pt-4">
    <button type="button" class="btn" onclick={test} disabled={testing || !url.trim()}>
      {#if testing}<LoaderCircle size={15} class="animate-spin" />{:else}<Plug size={15} />{/if} Test connection
    </button>
    <div class="ml-auto flex gap-2">
      {#if oncancel}<button type="button" class="btn btn-ghost" onclick={oncancel}>Cancel</button>{/if}
      <button class="btn btn-primary" disabled={saving || !canSubmit}>{#if saving}<LoaderCircle size={15} class="animate-spin" />{/if}{submitLabel}</button>
    </div>
  </div>
</form>
