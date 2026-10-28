<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import { toast, toastError } from "../../lib/state.svelte";

  let current = $state("");
  let next = $state("");
  let confirmPw = $state("");
  let busy = $state(false);

  const mismatch = $derived(confirmPw.length > 0 && confirmPw !== next);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (next !== confirmPw || next.length < 8) return;
    busy = true;
    try {
      await api.updateUser("me", { password: next, current_password: current });
      toast("ok", "Password changed");
      current = next = confirmPw = "";
    } catch (err) {
      toastError(err, "Could not change password");
    } finally {
      busy = false;
    }
  }
</script>

<form class="card space-y-4 p-5 sm:p-6" onsubmit={submit}>
  <div>
    <h2 class="card-title">Change password</h2>
    <p class="mt-1 text-xs text-fg-3">At least 8 characters.</p>
  </div>
  <div class="max-w-sm">
    <label class="label" for="cp-cur">Current password</label>
    <input id="cp-cur" class="input" type="password" autocomplete="current-password" bind:value={current} required />
  </div>
  <div class="max-w-sm">
    <label class="label" for="cp-new">New password</label>
    <input id="cp-new" class="input" type="password" autocomplete="new-password" minlength="8" bind:value={next} required />
  </div>
  <div class="max-w-sm">
    <label class="label" for="cp-conf">Confirm new password</label>
    <input id="cp-conf" class="input" type="password" autocomplete="new-password" bind:value={confirmPw} required />
    {#if mismatch}<p class="hint text-bad">Passwords don't match.</p>{/if}
  </div>
  <div class="flex justify-end">
    <button class="btn btn-primary" disabled={busy || mismatch || next.length < 8}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Change password</button>
  </div>
</form>
