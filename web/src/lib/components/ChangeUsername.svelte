<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import { api } from "../api";
  import { refreshSession, session, toast, toastError } from "../state.svelte";
  import Modal from "./Modal.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let name = $state("");
  let busy = $state(false);
  const current = $derived(session.me?.user?.username ?? "");

  $effect(() => {
    if (open) name = current;
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      await api.updateUser("me", { username: name.trim() });
      await refreshSession();
      toast("ok", "Username changed");
      open = false;
    } catch (err) {
      toastError(err, "Could not change username");
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="Change username" width="max-w-sm">
  <form id="chname" onsubmit={submit}>
    <label class="label" for="cn-name">New username</label>
    <input id="cn-name" class="input" bind:value={name} required maxlength="64" autocomplete="username" />
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" form="chname" disabled={busy || !name.trim() || name.trim() === current}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Change username</button>
  {/snippet}
</Modal>
