<script lang="ts">
  import { KeyRound, Zap } from "@lucide/svelte";
  import CodeBlock from "./CodeBlock.svelte";

  let { collectorUrl, compact = false }: { collectorUrl: string; compact?: boolean } = $props();

  let user = $state("monit");
  let pass = $state("");
  let httpdUser = $state("admin");
  let httpdPass = $state("");

  const url = $derived.by(() => {
    try {
      const u = new URL(collectorUrl);
      u.username = encodeURIComponent(user || "USER");
      u.password = encodeURIComponent(pass || "PASSWORD");
      return u.toString();
    } catch {
      return collectorUrl;
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

<div class="space-y-4">
  <div class="grid gap-4 {compact ? '' : 'sm:grid-cols-2'}">
    <div class="space-y-2.5">
      <div class="flex items-center gap-2 text-[13px] font-medium text-fg"><KeyRound size={14} class="text-accent" /> Collector credentials</div>
      {#if !compact}<p class="text-xs text-fg-3">Any Monarch user can post reports. A dedicated user with the <span class="font-medium text-fg-2">collector</span> role is recommended.</p>{/if}
      <div class="grid grid-cols-2 gap-2">
        <input class="input" placeholder="username" bind:value={user} aria-label="Collector username" />
        <input class="input" type="password" placeholder="password" bind:value={pass} aria-label="Collector password" autocomplete="off" />
      </div>
    </div>
    <div class="space-y-2.5">
      <div class="flex items-center gap-2 text-[13px] font-medium text-fg"><Zap size={14} class="text-accent-2" /> Monit httpd credentials</div>
      {#if !compact}<p class="text-xs text-fg-3">Required for service actions. Monarch must be able to reach port 2812 on the agent.</p>{/if}
      <div class="grid grid-cols-2 gap-2">
        <input class="input" placeholder="username" bind:value={httpdUser} aria-label="httpd username" />
        <input class="input" type="password" placeholder="password" bind:value={httpdPass} aria-label="httpd password" autocomplete="off" />
      </div>
    </div>
  </div>
  <p class="text-[11px] text-fg-3">These fields only fill in the snippet — nothing is saved or sent.</p>
  <CodeBlock code={snippet} label="/etc/monitrc" />
</div>
