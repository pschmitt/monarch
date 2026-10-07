<script lang="ts">
  import { Radio, Server } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import type { Settings } from "../../lib/types";
  import { fleet, toastError } from "../../lib/state.svelte";
  import CodeBlock from "../../lib/components/CodeBlock.svelte";
  import CollectorSnippet from "../../lib/components/CollectorSnippet.svelte";

  let s = $state<Settings | null>(null);
  $effect(() => {
    api.settings().then((v) => (s = v)).catch((e) => toastError(e, "Failed to load settings"));
  });

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
    {#if s}<CollectorSnippet collectorUrl={s.collector_url} />{/if}
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
    set mmonit ${(s?.collector_url ?? "").replace("://", "://USER:PASSWORD@")}
    set httpd port 2812 allow admin:SECRET
  '';
};`}
    />
  </section>
</div>
