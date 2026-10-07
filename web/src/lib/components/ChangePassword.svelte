<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import { api } from "../api";
  import { toast, toastError } from "../state.svelte";
  import Modal from "./Modal.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let current = $state("");
  let next = $state("");
  let confirmPw = $state("");
  let busy = $state(false);

  $effect(() => {
    if (open) current = next = confirmPw = "";
  });

  const mismatch = $derived(confirmPw.length > 0 && confirmPw !== next);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (next !== confirmPw || next.length < 8) return;
    busy = true;
    try {
      await api.updateUser("me", { password: next, current_password: current });
      toast("ok", "Password changed");
      open = false;
    } catch (err) {
      toastError(err, "Could not change password");
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="Change password" width="max-w-sm">
  <form id="chpw" class="space-y-4" onsubmit={submit}>
    <div>
      <label class="label" for="cp-cur">Current password</label>
      <input id="cp-cur" class="input" type="password" autocomplete="current-password" bind:value={current} required />
    </div>
    <div>
      <label class="label" for="cp-new">New password</label>
      <input id="cp-new" class="input" type="password" autocomplete="new-password" minlength="8" bind:value={next} required />
    </div>
    <div>
      <label class="label" for="cp-conf">Confirm new password</label>
      <input id="cp-conf" class="input" type="password" autocomplete="new-password" bind:value={confirmPw} required />
      {#if mismatch}<p class="hint text-bad">Passwords don't match.</p>{/if}
    </div>
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" form="chpw" disabled={busy || mismatch || next.length < 8}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Change password</button>
  {/snippet}
</Modal>
