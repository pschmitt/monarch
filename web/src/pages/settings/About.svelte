<script lang="ts">
  import { ExternalLink, Heart, Scale } from "@lucide/svelte";
  import { session } from "../../lib/state.svelte";
  import Logo from "../../lib/components/Logo.svelte";
</script>

<div class="space-y-6">
  <section class="card relative overflow-hidden p-6 sm:p-8">
    <div class="pointer-events-none absolute -top-24 -right-24 h-72 w-72 rounded-full bg-accent-gradient opacity-20 blur-3xl"></div>
    <div class="relative flex flex-wrap items-center gap-5">
      <Logo size={64} />
      <div>
        <h2 class="text-2xl font-semibold tracking-tight text-fg">Monarch</h2>
        <p class="mt-1 text-[13px] text-fg-2">A free, open source central dashboard for <a class="underline decoration-[var(--line-strong)] underline-offset-2 hover:text-fg" href="https://mmonit.com/monit/" data-external target="_blank" rel="noreferrer">Monit</a>.</p>
        <div class="num mt-2 text-xs text-fg-3">version {session.me?.version ?? "?"}</div>
      </div>
    </div>
  </section>

  <div class="grid gap-4 md:grid-cols-2">
    <section class="card space-y-2 p-5">
      <div class="flex items-center gap-2"><Scale size={15} class="text-accent" /><h3 class="card-title">License</h3></div>
      <p class="text-[13px] leading-relaxed text-fg-2">
        Monarch is free software, released under the <span class="font-medium text-fg">GNU General Public License v3.0</span>. You can run it on as many hosts as you like —
        no license keys, no host limits, no phoning home.
      </p>
      <a class="inline-flex items-center gap-1 text-xs text-fg-3 hover:text-fg" href="https://www.gnu.org/licenses/gpl-3.0.html" target="_blank" rel="noreferrer">Read the license <ExternalLink size={12} /></a>
    </section>
    <section class="card space-y-2 p-5">
      <div class="flex items-center gap-2"><Heart size={15} class="text-bad" /><h3 class="card-title">Compatibility</h3></div>
      <p class="text-[13px] leading-relaxed text-fg-2">
        Speaks Monit's M/Monit collector protocol, so existing <code class="num text-xs">set mmonit</code> lines just work. A subset of the M/Monit HTTP API is provided for
        existing tooling such as the Home Assistant integration.
      </p>
      <a class="inline-flex items-center gap-1 text-xs text-fg-3 hover:text-fg" href="https://github.com/pschmitt/monarch" target="_blank" rel="noreferrer">Source code <ExternalLink size={12} /></a>
    </section>
  </div>
</div>
