<script lang="ts">
  import { Check, Copy, KeyRound, LoaderCircle, Plus, Trash } from "@lucide/svelte";
  import { api } from "../api";
  import type { ApiToken } from "../types";
  import { ago, datetime } from "../format";
  import { clock, confirm, toast, toastError } from "../state.svelte";
  import Modal from "./Modal.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let tokens = $state<ApiToken[] | null>(null);
  let name = $state("");
  let days = $state("90");
  let busy = $state(false);
  let created = $state<string | null>(null);
  let copied = $state(false);

  async function load() {
    try {
      tokens = await api.tokens();
    } catch (e) {
      toastError(e, "Failed to load API tokens");
    }
  }

  $effect(() => {
    if (open) {
      created = null;
      copied = false;
      name = "";
      load();
    }
  });

  async function create(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      const t = await api.createToken(name.trim(), days === "never" ? null : Number(days));
      created = t.token;
      name = "";
      load();
    } catch (err) {
      toastError(err, "Could not create token");
    } finally {
      busy = false;
    }
  }

  async function copy() {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(created);
      copied = true;
    } catch {
      toast("warn", "Copy failed, select the token and copy it manually");
    }
  }

  async function revoke(t: ApiToken) {
    if (!(await confirm({ title: `Revoke ${t.name}?`, body: "Anything using this token stops working immediately.", confirm: "Revoke", danger: true }))) return;
    try {
      await api.revokeToken(t.id);
      load();
    } catch (err) {
      toastError(err, "Could not revoke token");
    }
  }
</script>

<Modal bind:open title="API tokens" width="max-w-xl">
  <div class="space-y-5">
    <p class="text-xs text-fg-3">
      Tokens act as you, with your role. Send one as <code class="num">Authorization: Bearer &lt;token&gt;</code>.
    </p>

    {#if created}
      <div class="rounded-xl border border-[color-mix(in_oklab,var(--ok)_40%,transparent)] bg-[color-mix(in_oklab,var(--ok)_8%,transparent)] p-3">
        <div class="mb-1.5 text-xs font-medium text-fg">Copy your new token now, it is not shown again.</div>
        <div class="flex items-center gap-2">
          <code class="num min-w-0 flex-1 truncate rounded-lg border border-line bg-surface-2 px-2.5 py-1.5 text-xs text-fg select-all">{created}</code>
          <button class="btn btn-sm" onclick={copy}>{#if copied}<Check size={14} /> Copied{:else}<Copy size={14} /> Copy{/if}</button>
        </div>
      </div>
    {/if}

    <form class="flex flex-wrap items-end gap-2" onsubmit={create}>
      <div class="min-w-40 flex-1">
        <label class="label" for="tk-name">Name</label>
        <input id="tk-name" class="input" bind:value={name} placeholder="Home Assistant" required maxlength="64" autocomplete="off" />
      </div>
      <div>
        <label class="label" for="tk-exp">Expires</label>
        <select id="tk-exp" class="input w-32" bind:value={days}>
          <option value="30">30 days</option>
          <option value="90">90 days</option>
          <option value="365">1 year</option>
          <option value="never">Never</option>
        </select>
      </div>
      <button class="btn btn-primary" disabled={busy || !name.trim()}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{:else}<Plus size={15} />{/if} Create</button>
    </form>

    {#if !tokens}
      <div class="skeleton h-10 w-full"></div>
    {:else if tokens.length === 0}
      <p class="text-center text-xs text-fg-3">No tokens yet.</p>
    {:else}
      <ul class="divide-y divide-[var(--line)] rounded-xl border border-line">
        {#each tokens as t (t.id)}
          <li class="flex items-center gap-3 px-3 py-2.5">
            <KeyRound size={15} class="shrink-0 text-fg-3" />
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-medium text-fg">{t.name}</div>
              <div class="num truncate text-[11px] text-fg-3">
                {t.prefix}… · {t.last_used ? `used ${ago(t.last_used, clock.now)}` : "never used"} ·
                {t.expires_at ? (t.expires_at < clock.now ? "expired" : `expires ${datetime(t.expires_at).split(",")[0]}`) : "no expiry"}
              </div>
            </div>
            <button class="btn btn-ghost btn-sm btn-icon text-bad" aria-label="Revoke {t.name}" onclick={() => revoke(t)}><Trash size={15} /></button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Modal>
