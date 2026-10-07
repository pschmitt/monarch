<script lang="ts">
  import { Ellipsis, KeyRound, LoaderCircle, Plus, Trash, UserPlus, Users } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { Role, User } from "../../lib/types";
  import { ago, datetime } from "../../lib/format";
  import { clock, confirm, session, toast, toastError } from "../../lib/state.svelte";
  import Empty from "../../lib/components/Empty.svelte";
  import Menu from "../../lib/components/Menu.svelte";
  import Modal from "../../lib/components/Modal.svelte";

  let users = $state<User[] | null>(null);
  let addOpen = $state(false);
  let pwUser = $state<User | null>(null);
  let pwOpen = $state(false);
  let form = $state({ username: "", password: "", role: "viewer" as Role });
  let newPw = $state("");
  let busy = $state(false);

  const roles: { id: Role; label: string; desc: string }[] = [
    { id: "admin", label: "Admin", desc: "Full access, including settings and users" },
    { id: "operator", label: "Operator", desc: "Read access plus service actions and acks" },
    { id: "viewer", label: "Viewer", desc: "Read-only access to the dashboard" },
    { id: "collector", label: "Collector", desc: "Can only submit Monit reports" },
  ];
  const roleTone: Record<Role, string> = { admin: "info", operator: "ok", viewer: "muted", collector: "warn" };

  async function load() {
    try {
      users = await api.users();
    } catch (e) {
      toastError(e, "Failed to load users");
    }
  }
  $effect(() => {
    load();
  });

  async function create(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      await api.createUser(form);
      toast("ok", `User ${form.username} created`);
      addOpen = false;
      form = { username: "", password: "", role: "viewer" };
      load();
    } catch (err) {
      toastError(err, "Could not create user");
    } finally {
      busy = false;
    }
  }

  async function setRole(u: User, role: Role) {
    try {
      await api.updateUser(u.id, { role });
      toast("ok", `${u.username} is now ${role}`);
      load();
    } catch (err) {
      toastError(err, "Could not change role");
    }
  }

  async function setPassword(e: SubmitEvent) {
    e.preventDefault();
    if (!pwUser) return;
    busy = true;
    try {
      await api.updateUser(pwUser.id, { password: newPw });
      toast("ok", `Password for ${pwUser.username} updated`);
      pwOpen = false;
      newPw = "";
    } catch (err) {
      toastError(err, "Could not update password");
    } finally {
      busy = false;
    }
  }

  async function remove(u: User) {
    if (!(await confirm({ title: `Delete ${u.username}?`, body: "Their sessions are revoked immediately. Monit agents using this account will stop reporting.", confirm: "Delete user", danger: true }))) return;
    try {
      await api.deleteUser(u.id);
      toast("ok", `${u.username} deleted`);
      load();
    } catch (err) {
      toastError(err, "Could not delete user");
    }
  }
</script>

<section class="card overflow-hidden">
  <div class="flex items-center justify-between gap-4 border-b border-line px-5 py-4">
    <div>
      <h2 class="card-title">Users</h2>
      <p class="mt-0.5 text-xs text-fg-3">People and agents that can access Monarch.</p>
    </div>
    <button class="btn btn-primary btn-sm" onclick={() => (addOpen = true)}><Plus size={14} /> Add user</button>
  </div>
  {#if !users}
    <div class="space-y-3 p-5">{#each Array(3) as _, i (i)}<div class="skeleton h-10 w-full"></div>{/each}</div>
  {:else if users.length === 0}
    <Empty icon={Users} title="No users" />
  {:else}
    <div class="overflow-x-auto">
      <table class="table min-w-[560px]">
        <thead><tr><th class="pl-5">User</th><th>Role</th><th>Last sign-in</th><th>Created</th><th class="pr-5"></th></tr></thead>
        <tbody>
          {#each users as u (u.id)}
            <tr>
              <td class="pl-5">
                <div class="flex items-center gap-3">
                  <span class="bg-accent-gradient flex h-8 w-8 items-center justify-center rounded-full text-[11px] font-bold text-white uppercase">{u.username.slice(0, 2)}</span>
                  <div>
                    <div class="flex items-center gap-1.5">
                      <span class="font-medium text-fg">{u.username}</span>
                      {#if u.auth_source === "oidc"}
                        <span class="inline-flex items-center gap-1 rounded-md border border-[color-mix(in_oklab,var(--accent)_35%,transparent)] bg-[color-mix(in_oklab,var(--accent)_10%,transparent)] px-1.5 py-px text-[10px] font-semibold text-accent" title="Signs in via {session.me?.oidc?.name ?? 'single sign-on'}"><KeyRound size={10} /> SSO</span>
                      {/if}
                    </div>
                    {#if u.id === session.me?.user?.id}<div class="text-[11px] text-accent">you</div>{/if}
                  </div>
                </div>
              </td>
              <td>
                <select
                  class="input tone-{roleTone[u.role]} h-8 w-32 text-xs"
                  value={u.role}
                  disabled={u.id === session.me?.user?.id}
                  onchange={(e) => setRole(u, (e.currentTarget as HTMLSelectElement).value as Role)}
                  aria-label="Role for {u.username}"
                >
                  {#each roles as r (r.id)}<option value={r.id}>{r.label}</option>{/each}
                </select>
                {#if u.auth_source === "oidc"}<div class="mt-1 text-[10px] text-fg-3">re-synced from IdP groups at sign-in</div>{/if}
              </td>
              <td class="num text-xs text-fg-2" title={datetime(u.last_login)}>{u.last_login ? ago(u.last_login, clock.now) : "never"}</td>
              <td class="num text-xs text-fg-3">{datetime(u.created_at).split(",")[0]}</td>
              <td class="pr-5 text-right">
                <Menu
                  label="Actions for {u.username}"
                  items={[
                    {
                      label: u.auth_source === "oidc" ? "Password managed by SSO" : "Set password",
                      icon: KeyRound,
                      disabled: u.auth_source === "oidc",
                      onselect: () => {
                        pwUser = u;
                        newPw = "";
                        pwOpen = true;
                      },
                    },
                    "sep",
                    { label: "Delete user", icon: Trash, danger: true, disabled: u.id === session.me?.user?.id, onselect: () => remove(u) },
                  ]}
                >
                  {#snippet trigger()}<Ellipsis size={16} />{/snippet}
                </Menu>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<Modal bind:open={addOpen} title="Add user">
  <form id="add-user" class="space-y-4" onsubmit={create}>
    <div>
      <label class="label" for="nu">Username</label>
      <input id="nu" class="input" bind:value={form.username} required autocomplete="off" />
    </div>
    <div>
      <label class="label" for="np">Password</label>
      <input id="np" class="input" type="password" bind:value={form.password} required minlength="8" autocomplete="new-password" />
    </div>
    <fieldset>
      <legend class="label">Role</legend>
      <div class="grid gap-2 sm:grid-cols-2">
        {#each roles as r (r.id)}
          <label class="flex cursor-pointer gap-3 rounded-xl border p-3 transition-colors {form.role === r.id ? 'border-[color-mix(in_oklab,var(--accent)_60%,transparent)] bg-[color-mix(in_oklab,var(--accent)_8%,transparent)]' : 'border-line hover:border-line-strong'}">
            <input type="radio" name="role" value={r.id} bind:group={form.role} class="mt-0.5 accent-[var(--accent)]" />
            <span>
              <span class="block text-[13px] font-medium text-fg">{r.label}</span>
              <span class="block text-[11px] text-fg-3">{r.desc}</span>
            </span>
          </label>
        {/each}
      </div>
    </fieldset>
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => (addOpen = false)}>Cancel</button>
    <button class="btn btn-primary" form="add-user" disabled={busy}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{:else}<UserPlus size={15} />{/if} Create user</button>
  {/snippet}
</Modal>

<Modal bind:open={pwOpen} title="Set password for {pwUser?.username ?? ''}" width="max-w-sm">
  <form id="set-pw" onsubmit={setPassword}>
    <label class="label" for="pw">New password</label>
    <input id="pw" class="input" type="password" bind:value={newPw} required minlength="8" autocomplete="new-password" />
    <p class="hint">At least 8 characters. Existing sessions stay signed in.</p>
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => (pwOpen = false)}>Cancel</button>
    <button class="btn btn-primary" form="set-pw" disabled={busy}>Update password</button>
  {/snippet}
</Modal>
