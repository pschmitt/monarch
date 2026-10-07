<script lang="ts">
  import { ArrowRight, Eye, EyeOff, LoaderCircle, ShieldCheck } from "@lucide/svelte";
  import { api, isMock } from "../lib/api";
  import { refreshSession, session } from "../lib/state.svelte";
  import Logo from "../lib/components/Logo.svelte";

  let { setup = false }: { setup?: boolean } = $props();

  // svelte-ignore state_referenced_locally
  let username = $state(setup ? "admin" : "");
  let password = $state("");
  let confirm = $state("");
  let show = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const mismatch = $derived(setup && confirm.length > 0 && confirm !== password);
  const weak = $derived(setup && password.length > 0 && password.length < 8);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    if (setup && (password !== confirm || password.length < 8)) {
      error = password.length < 8 ? "Use at least 8 characters." : "Passwords don't match.";
      return;
    }
    busy = true;
    try {
      const user = setup ? await api.setup(username, password) : await api.login(username, password);
      session.me = { user, setup_required: false, version: session.me?.version ?? "" };
      refreshSession();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="relative flex min-h-dvh items-center justify-center overflow-hidden px-4 py-10">
  <!-- animated backdrop -->
  <div class="pointer-events-none absolute inset-0" aria-hidden="true">
    <div class="blob absolute -top-40 -left-40 h-[520px] w-[520px] rounded-full opacity-40 blur-3xl" style="background: radial-gradient(circle, var(--accent), transparent 65%)"></div>
    <div class="blob2 absolute -right-40 -bottom-48 h-[560px] w-[560px] rounded-full opacity-30 blur-3xl" style="background: radial-gradient(circle, var(--accent-2), transparent 65%)"></div>
    <div class="absolute inset-0 grid-bg"></div>
  </div>

  <div class="relative w-full max-w-[400px] animate-in">
    <div class="mb-8 flex flex-col items-center text-center">
      <div class="relative">
        <div class="absolute inset-0 -m-4 rounded-full bg-accent-gradient opacity-30 blur-2xl"></div>
        <div class="relative"><Logo size={56} /></div>
      </div>
      <h1 class="mt-5 text-2xl font-semibold tracking-tight text-fg">
        {#if setup}Welcome to <span class="text-gradient">Monarch</span>{:else}Sign in to <span class="text-gradient">Monarch</span>{/if}
      </h1>
      <p class="mt-2 text-[13px] text-fg-3">
        {#if setup}Create the first administrator account to get started.{:else}Fleet-wide monitoring for your Monit agents.{/if}
      </p>
    </div>

    <form class="card space-y-4 p-6" onsubmit={submit}>
      <div>
        <label class="label" for="u">Username</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="u" class="input" autocomplete="username" bind:value={username} required autofocus={!setup} />
      </div>
      <div>
        <label class="label" for="p">Password</label>
        <div class="relative">
          <input
            id="p"
            class="input pr-10"
            type={show ? "text" : "password"}
            autocomplete={setup ? "new-password" : "current-password"}
            bind:value={password}
            required
          />
          <button type="button" class="absolute top-1/2 right-2 -translate-y-1/2 p-1 text-fg-3 hover:text-fg" onclick={() => (show = !show)} aria-label={show ? "Hide password" : "Show password"}>
            {#if show}<EyeOff size={16} />{:else}<Eye size={16} />{/if}
          </button>
        </div>
        {#if weak}<p class="hint text-warn">At least 8 characters, please.</p>{/if}
      </div>
      {#if setup}
        <div>
          <label class="label" for="c">Confirm password</label>
          <input id="c" class="input" type={show ? "text" : "password"} autocomplete="new-password" bind:value={confirm} required />
          {#if mismatch}<p class="hint text-bad">Passwords don't match.</p>{/if}
        </div>
      {/if}

      {#if error}
        <div class="tone-bad rounded-xl border border-tone-soft bg-tone-soft px-3 py-2 text-[13px] text-tone animate-in">{error}</div>
      {/if}

      <button class="btn btn-primary h-10 w-full" disabled={busy}>
        {#if busy}<LoaderCircle size={16} class="animate-spin" />{/if}
        {setup ? "Create account" : "Sign in"}
        {#if !busy}<ArrowRight size={16} />{/if}
      </button>
      {#if isMock && !setup}
        <p class="text-center text-[11px] text-fg-3">Demo mode — any username and password (3+ chars) works.</p>
      {/if}
    </form>

    <p class="mt-6 flex items-center justify-center gap-1.5 text-[11px] text-fg-3">
      <ShieldCheck size={13} /> Free software · GPL-3.0 · no license keys, no limits
    </p>
  </div>
</div>

<style>
  .grid-bg {
    background-image:
      linear-gradient(var(--line) 1px, transparent 1px),
      linear-gradient(90deg, var(--line) 1px, transparent 1px);
    background-size: 48px 48px;
    mask-image: radial-gradient(ellipse at center, black 20%, transparent 70%);
    opacity: 0.6;
  }
  .blob {
    animation: drift 18s ease-in-out infinite alternate;
  }
  .blob2 {
    animation: drift 22s ease-in-out infinite alternate-reverse;
  }
  @keyframes drift {
    from {
      transform: translate(0, 0) scale(1);
    }
    to {
      transform: translate(120px, 80px) scale(1.15);
    }
  }
</style>
