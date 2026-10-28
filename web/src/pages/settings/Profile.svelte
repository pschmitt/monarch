<script lang="ts">
  import { ImageUp, LoaderCircle, Save } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import { refreshSession, session, toast, toastError } from "../../lib/state.svelte";
  import Avatar from "../../lib/components/Avatar.svelte";
  import ProfilePicture from "../../lib/components/ProfilePicture.svelte";

  const user = $derived(session.me?.user ?? null);
  const current = $derived(user?.username ?? "");
  const currentEmail = $derived(user?.email ?? "");
  // Single sign-on accounts get their name and email from the provider.
  const editable = $derived(!user?.sso);

  let name = $state("");
  let email = $state("");
  let busy = $state(false);
  let pictureOpen = $state(false);
  const changed = $derived(name.trim() !== current || email.trim() !== currentEmail);

  $effect(() => {
    name = current;
    email = currentEmail;
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
    } catch (err) {
      toastError(err, "Could not update profile");
    } finally {
      busy = false;
    }
  }
</script>

<div class="space-y-6">
  <section class="card p-5 sm:p-6">
    <div class="flex flex-wrap items-center gap-5">
      <Avatar {user} size={72} />
      <div class="min-w-0 flex-1">
        <h2 class="card-title">Profile picture</h2>
        <p class="mt-1 text-xs text-fg-3">Visible to everyone signed in to Monarch.{#if user?.avatar === 0} Currently shown: Gravatar.{/if}</p>
      </div>
      <button class="btn" onclick={() => (pictureOpen = true)}><ImageUp size={15} /> Change…</button>
    </div>
  </section>

  <form class="card space-y-4 p-5 sm:p-6" onsubmit={submit}>
    <div>
      <h2 class="card-title">Profile</h2>
      <p class="mt-1 text-xs text-fg-3">
        Role: <span class="text-fg-2 capitalize">{user?.role}</span>{#if user?.sso} · managed by {session.me?.oidc?.name ?? "single sign-on"}{/if}
      </p>
    </div>
    <div class="max-w-sm">
      <label class="label" for="ac-name">Username</label>
      <input id="ac-name" class="input" bind:value={name} required maxlength="64" autocomplete="username" disabled={!editable} />
    </div>
    <div class="max-w-sm">
      <label class="label" for="ac-email">Email</label>
      <input id="ac-email" class="input" type="email" bind:value={email} placeholder="you@example.com" autocomplete="email" disabled={!editable} />
      <p class="hint">Used for notifications addressed to your role. {editable ? "Leave empty to remove it." : `Comes from ${session.me?.oidc?.name ?? "single sign-on"}.`}</p>
    </div>
    {#if editable}
      <div class="flex justify-end">
        <button class="btn btn-primary" disabled={busy || !name.trim() || !changed}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{:else}<Save size={15} />{/if} Save</button>
      </div>
    {/if}
  </form>
</div>

<ProfilePicture bind:open={pictureOpen} />
