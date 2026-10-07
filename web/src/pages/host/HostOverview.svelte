<script lang="ts">
  import { HardDrive, Info, Network } from "@lucide/svelte";
  import type { HostDetail } from "../../lib/types";
  import { datetime, duration, kb, mb, pct, rate, usageTone } from "../../lib/format";
  import { serviceHref } from "../../lib/router.svelte";
  import { systemCharts } from "../../lib/service";
  import KV from "../../lib/components/KV.svelte";
  import MetricChart from "../../lib/components/MetricChart.svelte";
  import RangePicker from "../../lib/components/RangePicker.svelte";
  import StatusDot from "../../lib/components/StatusDot.svelte";
  import UsageBar from "../../lib/components/UsageBar.svelte";

  let { host }: { host: HostDetail } = $props();

  let range = $state(localStorage.getItem("monarch.range") ?? "6h");
  $effect(() => {
    try {
      localStorage.setItem("monarch.range", range);
    } catch {
      /* ignore */
    }
  });

  const system = $derived(host.services.find((s) => s.type === "system"));
  const filesystems = $derived(host.services.filter((s) => s.type === "filesystem").sort((a, b) => (b.data.space?.percent ?? 0) - (a.data.space?.percent ?? 0)));
  const nets = $derived(host.services.filter((s) => s.type === "net"));
  const sys = $derived(system?.data ?? {});
  const charts = systemCharts();
  const current = (key: string): string | null => {
    if (key === "cpu_user") return pct(sys.cpu?.total);
    if (key === "mem_percent") return pct(sys.memory?.percent);
    if (key === "load1") return sys.load ? sys.load[0].toFixed(2) : null;
    return null;
  };
</script>

<div class="space-y-6">
  {#if system}
    <div class="flex flex-wrap items-center justify-between gap-3">
      <h2 class="card-title">System</h2>
      <RangePicker bind:value={range} />
    </div>
    <div class="grid gap-4 lg:grid-cols-3">
      {#each charts as c (c.title)}
        <MetricChart host={host.id} service={system.name} title={c.title} metrics={c.metrics} format={c.format} stacked={c.stacked} yMax={c.yMax ?? null} {range} current={current(c.metrics[0].key)} />
      {/each}
    </div>
  {/if}

  <div class="grid items-start gap-4 lg:grid-cols-3">
    <!-- filesystems -->
    <section class="card overflow-hidden lg:col-span-2">
      <div class="flex items-center gap-2 border-b border-line px-5 py-3.5">
        <HardDrive size={15} class="text-fg-3" />
        <h2 class="card-title">Filesystems</h2>
      </div>
      {#if filesystems.length === 0}
        <p class="px-5 py-6 text-[13px] text-fg-3">No filesystem checks configured.</p>
      {:else}
        <ul class="divide-y divide-[var(--line)]">
          {#each filesystems as f (f.id)}
            {@const sp = f.data.space ?? {}}
            <li>
              <a href={serviceHref(host.id, f.name)} class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 px-5 py-3 hover:bg-hover sm:grid-cols-[180px_minmax(0,1fr)_auto]">
                <div class="min-w-0">
                  <div class="flex items-center gap-2 truncate text-[13px] font-medium text-fg"><StatusDot tone={f.state === "failed" ? "bad" : usageTone(sp.percent)} size={6} />{f.name}</div>
                  <div class="num truncate text-[11px] text-fg-3">{f.data.fstype ?? ""}</div>
                </div>
                <div class="col-span-2 row-start-2 sm:col-span-1 sm:row-start-auto">
                  <UsageBar value={sp.percent} height={7} />
                  {#if f.data.inodes}<div class="mt-1"><UsageBar value={f.data.inodes.percent} height={3} tone="info" /></div>{/if}
                </div>
                <div class="num text-right text-xs">
                  <div class="font-semibold text-fg">{pct(sp.percent)}</div>
                  <div class="text-[11px] text-fg-3">{mb(sp.used_mb)} / {mb(sp.total_mb)}</div>
                </div>
              </a>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <!-- monit info -->
    <section class="card p-5">
      <div class="mb-4 flex items-center gap-2">
        <Info size={15} class="text-fg-3" />
        <h2 class="card-title">Monit agent</h2>
      </div>
      <dl class="grid grid-cols-2 gap-x-4 gap-y-4">
        <KV label="Version" mono>{host.monit_version ?? "—"}</KV>
        <KV label="Agent uptime" mono>{duration(host.monit_uptime)}</KV>
        <KV label="Poll interval" mono>{host.poll}s</KV>
        <KV label="Start delay" mono>{host.startdelay ?? 0}s</KV>
        <KV label="Memory" mono>{kb(host.mem_total_kb, 0)}</KV>
        <KV label="Swap" mono>{host.swap_total_kb ? kb(host.swap_total_kb, 0) : "none"}</KV>
        <KV label="First seen">{datetime(host.first_seen)}</KV>
        <KV label="Reports from" mono>{host.remote_addr ?? "—"}</KV>
        <div class="col-span-2"><KV label="Control file" mono>{host.controlfile ?? "—"}</KV></div>
        <div class="col-span-2"><KV label="Actions via" mono>{host.monit_url ?? "unavailable — no httpd"}</KV></div>
        <div class="col-span-2"><KV label="Monit ID" mono><span class="text-[11px]">{host.monit_id}</span></KV></div>
      </dl>
    </section>
  </div>

  {#if nets.length}
    <div>
      <div class="mb-3 flex items-center gap-2">
        <Network size={15} class="text-fg-3" />
        <h2 class="card-title">Network</h2>
      </div>
      <div class="grid gap-4 lg:grid-cols-2">
        {#each nets as n (n.id)}
          <MetricChart
            host={host.id}
            service={n.name}
            title={n.name}
            metrics={[
              { key: "rx_bps", label: "Download", color: "var(--accent-2)" },
              { key: "tx_bps", label: "Upload", color: "var(--accent)" },
            ]}
            format={(v) => rate(v)}
            {range}
            height={150}
            current={`↓ ${rate(n.data.download?.bytes)}`}
          />
        {/each}
      </div>
    </div>
  {/if}
</div>
