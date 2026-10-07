<script lang="ts">
  import { BellRing, Hash, LoaderCircle, Mail, MessageCircle, Pencil, Plus, Send, Trash, Webhook } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { Channel, ChannelKind, EventState } from "../../lib/types";
  import { ago, eventStateLabel, eventTone } from "../../lib/format";
  import { clock, confirm, toast, toastError } from "../../lib/state.svelte";
  import Empty from "../../lib/components/Empty.svelte";
  import Modal from "../../lib/components/Modal.svelte";
  import StatusDot from "../../lib/components/StatusDot.svelte";
  import Switch from "../../lib/components/Switch.svelte";

  interface Field {
    key: string;
    label: string;
    placeholder?: string;
    secret?: boolean;
    multiline?: boolean;
    hint?: string;
  }

  const KINDS: Record<ChannelKind, { label: string; icon: any; fields: Field[] }> = {
    ntfy: {
      label: "ntfy",
      icon: BellRing,
      fields: [
        { key: "url", label: "Topic URL", placeholder: "https://ntfy.sh/my-monarch-alerts" },
        { key: "token", label: "Access token", secret: true, hint: "Optional" },
      ],
    },
    gotify: {
      label: "Gotify",
      icon: BellRing,
      fields: [
        { key: "url", label: "Server URL", placeholder: "https://gotify.example.com" },
        { key: "token", label: "App token", secret: true },
      ],
    },
    slack: { label: "Slack", icon: Hash, fields: [{ key: "url", label: "Incoming webhook URL", placeholder: "https://hooks.slack.com/services/…", secret: true }] },
    discord: { label: "Discord", icon: MessageCircle, fields: [{ key: "url", label: "Webhook URL", placeholder: "https://discord.com/api/webhooks/…", secret: true }] },
    telegram: {
      label: "Telegram",
      icon: Send,
      fields: [
        { key: "token", label: "Bot token", secret: true },
        { key: "chat_id", label: "Chat ID", placeholder: "-1001234567890" },
      ],
    },
    email: {
      label: "Email",
      icon: Mail,
      fields: [
        { key: "smtp_url", label: "SMTP URL", placeholder: "smtps://user:pass@smtp.example.com:465", secret: true },
        { key: "from", label: "From", placeholder: "monarch@example.com" },
        { key: "to", label: "To", placeholder: "ops@example.com, oncall@example.com", hint: "Comma separated" },
      ],
    },
    webhook: {
      label: "Webhook",
      icon: Webhook,
      fields: [
        { key: "url", label: "URL", placeholder: "https://example.com/hooks/monarch" },
        { key: "method", label: "Method", placeholder: "POST" },
        { key: "headers", label: "Headers", placeholder: "Authorization: Bearer …", multiline: true, secret: true, hint: "One “Key: value” per line. Body is the event as JSON." },
      ],
    },
  };
  const MASK = "********";
  const STATES: EventState[] = ["failed", "succeeded", "changed", "changed_not"];

  let channels = $state<Channel[] | null>(null);
  let open = $state(false);
  let editing = $state<Channel | null>(null);
  let draft = $state<{ name: string; kind: ChannelKind; enabled: boolean; config: Record<string, string>; filter: Channel["filter"] }>(blank());
  let busy = $state(false);
  let testing = $state<number | null>(null);

  function blank() {
    return {
      name: "",
      kind: "ntfy" as ChannelKind,
      enabled: true,
      config: {} as Record<string, string>,
      filter: { hosts: null, services: null, states: ["failed", "succeeded"] as EventState[], include_heartbeat: true },
    };
  }

  async function load() {
    try {
      channels = await api.channels();
    } catch (e) {
      toastError(e, "Failed to load channels");
    }
  }
  $effect(() => {
    load();
  });

  function edit(c: Channel | null) {
    editing = c;
    const config = { ...(c?.config ?? {}) };
    // Masked secrets are shown as empty inputs with an "unchanged" placeholder.
    if (c) for (const f of KINDS[c.kind].fields) if (f.secret && config[f.key] === MASK) config[f.key] = "";
    draft = c ? { name: c.name, kind: c.kind, enabled: c.enabled, config, filter: { ...c.filter, states: [...c.filter.states] } } : blank();
    open = true;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      const config = { ...draft.config };
      // Send the mask back for untouched secrets so the backend keeps the stored value.
      if (editing) for (const f of KINDS[draft.kind].fields) if (f.secret && !config[f.key] && editing.config[f.key] === MASK) config[f.key] = MASK;
      const body = { ...draft, config, filter: { ...draft.filter, hosts: draft.filter.hosts || null, services: draft.filter.services || null } };
      if (editing) await api.updateChannel(editing.id, body);
      else await api.createChannel(body);
      toast("ok", editing ? "Channel updated" : "Channel created");
      open = false;
      load();
    } catch (err) {
      toastError(err, "Could not save channel");
    } finally {
      busy = false;
    }
  }

  async function toggleEnabled(c: Channel, v: boolean) {
    try {
      await api.updateChannel(c.id, { enabled: v });
      c.enabled = v;
    } catch (err) {
      toastError(err, "Could not update channel");
      load();
    }
  }

  async function test(c: Channel) {
    testing = c.id;
    try {
      const r = await api.testChannel(c.id);
      toast(r.ok ? "ok" : "bad", r.ok ? `Test sent via ${c.name}` : `Test via ${c.name} failed`, r.message);
      load();
    } catch (err) {
      toastError(err, "Test failed");
    } finally {
      testing = null;
    }
  }

  async function remove(c: Channel) {
    if (!(await confirm({ title: `Delete ${c.name}?`, body: "Notifications will no longer be delivered to this channel.", confirm: "Delete", danger: true }))) return;
    try {
      await api.deleteChannel(c.id);
      load();
    } catch (err) {
      toastError(err, "Could not delete channel");
    }
  }

  function toggleState(s: EventState) {
    draft.filter.states = draft.filter.states.includes(s) ? draft.filter.states.filter((x) => x !== s) : [...draft.filter.states, s];
  }
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h2 class="card-title">Notification channels</h2>
      <p class="mt-0.5 text-xs text-fg-3">Where Monarch sends alerts when services fail, recover, or hosts stop reporting.</p>
    </div>
    <button class="btn btn-primary btn-sm" onclick={() => edit(null)}><Plus size={14} /> Add channel</button>
  </div>

  {#if !channels}
    <div class="grid gap-3 md:grid-cols-2">{#each Array(2) as _, i (i)}<div class="skeleton h-36 rounded-2xl"></div>{/each}</div>
  {:else if channels.length === 0}
    <div class="card">
      <Empty icon={BellRing} title="No channels yet" body="Add ntfy, Slack, Discord, Telegram, Gotify, email or a generic webhook.">
        <button class="btn btn-primary" onclick={() => edit(null)}><Plus size={14} /> Add channel</button>
      </Empty>
    </div>
  {:else}
    <div class="grid gap-3 md:grid-cols-2">
      {#each channels as c (c.id)}
        {@const K = KINDS[c.kind]}
        <div class="card flex flex-col p-4 {c.enabled ? '' : 'opacity-65'}">
          <div class="flex items-start gap-3">
            <span class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-line-strong bg-surface-2 text-accent"><K.icon size={18} /></span>
            <div class="min-w-0 flex-1">
              <div class="truncate text-[14px] font-semibold text-fg">{c.name}</div>
              <div class="text-xs text-fg-3">{K.label}</div>
            </div>
            <Switch checked={c.enabled} label="Enabled" onchange={(v) => toggleEnabled(c, v)} />
          </div>
          <div class="mt-3 flex flex-wrap gap-1.5">
            {#each c.filter.states as s (s)}
              <span class="tone-{eventTone(s)} rounded-md bg-tone-soft px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-tone uppercase">{eventStateLabel[s]}</span>
            {/each}
            {#if c.filter.include_heartbeat}<span class="rounded-md border border-line px-1.5 py-0.5 text-[10px] text-fg-3">heartbeat</span>{/if}
            {#if c.filter.hosts}<span class="num rounded-md border border-line px-1.5 py-0.5 text-[10px] text-fg-3">hosts ~ {c.filter.hosts}</span>{/if}
            {#if c.filter.services}<span class="num rounded-md border border-line px-1.5 py-0.5 text-[10px] text-fg-3">services ~ {c.filter.services}</span>{/if}
          </div>
          <div class="mt-auto flex items-center gap-2 border-t border-line pt-3 mt-4 text-[11px]">
            {#if c.last_sent_at}
              <StatusDot tone={c.last_status === "ok" ? "ok" : "bad"} size={6} />
              <span class="truncate {c.last_status === 'ok' ? 'text-fg-3' : 'text-bad'}">{c.last_status === "ok" ? "delivered" : c.last_status} · {ago(c.last_sent_at, clock.now)}</span>
            {:else}<span class="text-fg-3">never used</span>{/if}
            <div class="ml-auto flex gap-1">
              <button class="btn btn-ghost btn-sm" onclick={() => test(c)} disabled={testing === c.id}>{#if testing === c.id}<LoaderCircle size={13} class="animate-spin" />{:else}<Send size={13} />{/if} Test</button>
              <button class="btn btn-ghost btn-sm btn-icon" onclick={() => edit(c)} aria-label="Edit {c.name}"><Pencil size={13} /></button>
              <button class="btn btn-ghost btn-sm btn-icon hover:text-bad" onclick={() => remove(c)} aria-label="Delete {c.name}"><Trash size={13} /></button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<Modal bind:open title={editing ? `Edit ${editing.name}` : "New notification channel"} width="max-w-xl">
  <form id="chan" class="space-y-5" onsubmit={save}>
    {#if !editing}
      <div>
        <span class="label">Type</span>
        <div class="grid grid-cols-4 gap-2 sm:grid-cols-7">
          {#each Object.entries(KINDS) as [k, K] (k)}
            <button
              type="button"
              class="flex flex-col items-center gap-1.5 rounded-xl border px-1 py-2.5 text-[11px] transition-colors {draft.kind === k ? 'border-[color-mix(in_oklab,var(--accent)_60%,transparent)] bg-[color-mix(in_oklab,var(--accent)_10%,transparent)] text-fg' : 'border-line text-fg-3 hover:border-line-strong hover:text-fg-2'}"
              onclick={() => {
                draft.kind = k as ChannelKind;
                draft.config = {};
              }}
            >
              <K.icon size={17} />{K.label}
            </button>
          {/each}
        </div>
      </div>
    {/if}
    <div>
      <label class="label" for="cn">Name</label>
      <input id="cn" class="input" bind:value={draft.name} placeholder="{KINDS[draft.kind].label} alerts" required />
    </div>
    {#each KINDS[draft.kind].fields as f (f.key)}
      <div>
        <label class="label" for="f-{f.key}">{f.label}</label>
        {#if f.multiline}
          <textarea id="f-{f.key}" class="input num text-xs" rows="3" placeholder={f.placeholder} bind:value={draft.config[f.key]}></textarea>
        {:else}
          <input id="f-{f.key}" class="input {f.secret ? '' : 'num'}" type={f.secret ? "password" : "text"} autocomplete="off" placeholder={editing && f.secret ? "•••••••• (unchanged)" : f.placeholder} bind:value={draft.config[f.key]} />
        {/if}
        {#if f.hint}<p class="hint">{f.hint}</p>{/if}
      </div>
    {/each}

    <div class="space-y-3 rounded-xl border border-line p-4">
      <div class="text-[13px] font-medium text-fg">Routing</div>
      <div class="flex flex-wrap gap-2">
        {#each STATES as s (s)}
          <button
            type="button"
            class="tone-{eventTone(s)} rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors {draft.filter.states.includes(s) ? 'border-tone-soft bg-tone-soft text-tone' : 'border-line text-fg-3'}"
            onclick={() => toggleState(s)}
            aria-pressed={draft.filter.states.includes(s)}>{eventStateLabel[s]}</button
          >
        {/each}
      </div>
      <div class="grid gap-3 sm:grid-cols-2">
        <div>
          <label class="label" for="fh">Hosts (regex)</label>
          <input id="fh" class="input num text-xs" placeholder="any host" bind:value={draft.filter.hosts} />
        </div>
        <div>
          <label class="label" for="fs">Services (regex)</label>
          <input id="fs" class="input num text-xs" placeholder="any service" bind:value={draft.filter.services} />
        </div>
      </div>
      <label class="flex items-center justify-between gap-4 text-[13px] text-fg-2">
        Notify when hosts stop reporting (heartbeat)
        <Switch bind:checked={draft.filter.include_heartbeat} label="Heartbeat alerts" />
      </label>
    </div>
  </form>
  {#snippet footer()}
    <label class="mr-auto flex items-center gap-2 text-xs text-fg-2"><Switch bind:checked={draft.enabled} label="Enabled" /> Enabled</label>
    <button class="btn btn-ghost" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" form="chan" disabled={busy}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{/if}{editing ? "Save" : "Create channel"}</button>
  {/snippet}
</Modal>
