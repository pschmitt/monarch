<script lang="ts">
  import { KeyRound, Radio, Server, Zap } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { Settings } from "../../lib/types";
  import { fleet, toastError } from "../../lib/state.svelte";
  import CodeBlock from "../../lib/components/CodeBlock.svelte";

  let s = $state<Settings | null>(null);
  let user = $state("monit");
  let pass = $state("");
  let httpdUser = $state("admin");
  let httpdPass = $state("");

  $effect(() => {
    api.settings().then((v) => (s = v)).catch((e) => toastError(e, "Failed to load settings"));
  });

  const url = $derived.by(() => {
    if (!s) return "";
    try {
      const u = new URL(s.collector_url);
      u.username = encodeURIComponent(user || "USER");
      u.password = encodeURIComponent(pass || "PASSWORD");
      return u.toString();
    } catch {
      return s.collector_url;
    }
  });

  const snippet = $derived(
    `# Report to Monarch
set mmonit ${url}
    with timeout 30 seconds

# Let Monarch start/stop/restart services.
# Monit registers the first "allow user:password" with Monarch.
set httpd port 2812
    allow ${httpdUser || "admin"}:${httpdPass || "SECRET"}`,
  );
</script>

<div class="space-y-6">
  <section class="card relative overflow-hidden p-5 sm:p-6">
    <div class="pointer-events-none absolute -top-20 -right-20 h-56 w-56 rounded-full bg-accent-gradient opacity-15 blur-3xl"></div>
    <div class="relative flex items-start gap-4">
      <div class="bg-accent-gradient flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl text-white shadow-lg"><Radio size={20} /></div>
      <div>
        <h2 class="text-base font-semibold text-fg">Connect a Monit agent</h2>
        <p class="mt-1 max-w-2xl text-[13px] leading-relaxed text-fg-2">
          Monarch speaks the M/Monit collector protocol. Add the snippet below to <code class="num text-xs">monitrc</code> and reload Monit — the host shows up within one poll cycle.
          Already using M/Monit? Just add a second <code class="num text-xs">set mmonit</code> line to run both side by side.
        </p>
      </div>
    </div>
    {#if s}
      <div class="num relative mt-5 flex flex-wrap items-center gap-2 rounded-xl border border-line bg-surface-2/60 px-3.5 py-2.5 text-xs">
        <span class="text-fg-3">Collector endpoint</span>
        <span class="text-fg">{s.collector_url}</span>
        <span class="ml-auto flex items-center gap-1.5 text-fg-3"><Server size={12} />{Object.keys(fleet.hosts).length} hosts reporting</span>
      </div>
    {/if}
  </section>

  <section class="card space-y-5 p-5 sm:p-6">
    <div class="grid gap-4 sm:grid-cols-2">
      <div class="space-y-3">
        <div class="flex items-center gap-2 text-[13px] font-medium text-fg"><KeyRound size={14} class="text-accent" /> Collector credentials</div>
        <p class="text-xs text-fg-3">Any Monarch user can post reports. A dedicated user with the <span class="font-medium text-fg-2">collector</span> role is recommended.</p>
        <div class="grid grid-cols-2 gap-2">
          <input class="input" placeholder="username" bind:value={user} aria-label="Collector username" />
          <input class="input" type="password" placeholder="password" bind:value={pass} aria-label="Collector password" autocomplete="off" />
        </div>
      </div>
      <div class="space-y-3">
        <div class="flex items-center gap-2 text-[13px] font-medium text-fg"><Zap size={14} class="text-accent-2" /> Monit httpd credentials</div>
        <p class="text-xs text-fg-3">Required for service actions. Monarch must be able to reach port 2812 on the agent.</p>
        <div class="grid grid-cols-2 gap-2">
          <input class="input" placeholder="username" bind:value={httpdUser} aria-label="httpd username" />
          <input class="input" type="password" placeholder="password" bind:value={httpdPass} aria-label="httpd password" autocomplete="off" />
        </div>
      </div>
    </div>
    <p class="text-[11px] text-fg-3">These fields only fill in the snippet — nothing is saved or sent.</p>
    <CodeBlock code={snippet} label="/etc/monitrc" />
    <div class="grid gap-3 text-xs text-fg-2 sm:grid-cols-3">
      <div class="rounded-xl border border-line p-3.5"><div class="mb-1 font-semibold text-fg">1 · Add the snippet</div>Paste it into monitrc (or an included file) on each host.</div>
      <div class="rounded-xl border border-line p-3.5"><div class="mb-1 font-semibold text-fg">2 · Reload Monit</div><code class="num">monit reload</code> or restart the service.</div>
      <div class="rounded-xl border border-line p-3.5"><div class="mb-1 font-semibold text-fg">3 · Watch it appear</div>The host shows up live on the overview — no refresh needed.</div>
    </div>
  </section>

  <section class="card space-y-3 p-5 sm:p-6">
    <h3 class="card-title">NixOS</h3>
    <p class="text-xs text-fg-3">With the NixOS <code class="num">services.monit</code> module:</p>
    <CodeBlock
      label="configuration.nix"
      code={`services.monit = {
  enable = true;
  config = ''
    set daemon 60
    set mmonit ${url}
    set httpd port 2812 allow ${httpdUser || "admin"}:${httpdPass || "SECRET"}
  '';
};`}
    />
  </section>
</div>
