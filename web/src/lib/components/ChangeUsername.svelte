<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import { api } from "../api";
  import { refreshSession, session, toast, toastError } from "../state.svelte";
  import Modal from "./Modal.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let name = $state("");
  let email = $state("");
  let busy = $state(false);
  const current = $derived(session.me?.user?.username ?? "");
  const currentEmail = $derived(session.me?.user?.email ?? "");
  const changed = $derived(name.trim() !== current || email.trim() !== currentEmail);

  $effect(() => {
    if (open) {
      name = current;
      email = currentEmail;
    }
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      const patch: Record<string, string> = {};
      if (name.trim() !== current) patch.username = name.trim();
      if (email.trim() !== currentEmail) patch.email = email.trim();
      await api.updateUser("me", patch);
      await refreshSession();
      toast("ok", "Profile updated");
      open = false;
    } catch (err) {
      toastError(err, "Could not update profile");
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="Edit profile" width="max-w-sm">
  <form id="chname" class="space-y-4" onsubmit={submit}>
    <div>
      <label class="label" for="cn-name">Username</label>
      <input id="cn-name" class="input" bind:value={name} required maxlength="64" autocomplete="username" />
    </div>
    <div>
      <label class="label" for="cn-email">Email</label>
      <input id="cn-email" class="input" type="email" bind:value={email} placeholder="you@example.com" autocomplete="email" />
      <p class="hint">Used for notifications addressed to your role. Leave empty to remove it.</p>
    </div>
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" form="chname" disabled={busy || !name.trim() || !changed}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Save</button>
  {/snippet}
</Modal>
