<script lang="ts">
  import { ArrowDown, ArrowUp, Lock, ShieldCheck, ShieldAlert, SquareTerminal } from "@lucide/svelte";
  import type { Port, Service, UnixSocket } from "../../lib/types";
  import { ago, bits, bytes, compact, datetime, duration, kb, mb, ms, num, octalMode, pct, rate } from "../../lib/format";
  import { clock } from "../../lib/state.svelte";
  import Gauge from "../../lib/components/Gauge.svelte";
  import UsageBar from "../../lib/components/UsageBar.svelte";

  let { service }: { service: Service } = $props();
  const d = $derived(service.data ?? {});
</script>

{#snippet stat(label: string, value: string, sub: string = "")}
  <div class="rounded-xl border border-line bg-surface-2/40 px-4 py-3">
    <div class="eyebrow">{label}</div>
    <div class="num mt-1 truncate text-lg font-semibold text-fg">{value}</div>
    {#if sub}<div class="num truncate text-[11px] text-fg-3">{sub}</div>{/if}
  </div>
{/snippet}

{#snippet ports(list: Port[], unix: UnixSocket[] = [])}
  {#if list.length || unix.length}
    <section class="card overflow-x-auto">
      <div class="border-b border-line px-5 py-3.5"><h3 class="card-title">Connection tests</h3></div>
      <table class="table min-w-[640px]">
        <thead><tr><th class="pl-5">Target</th><th>Protocol</th><th>Request</th><th>Response</th><th class="pr-5">Certificate</th></tr></thead>
        <tbody>
          {#each list as p, i (i)}
            <tr>
              <td class="num pl-5 text-fg">{p.hostname}:{p.port}</td>
              <td><span class="num text-xs text-fg-2">{p.protocol}/{p.type}</span></td>
              <td class="num max-w-48 truncate text-xs text-fg-3">{p.request ?? "—"}</td>
              <td class="num text-xs {p.response_ms === null ? 'text-bad' : 'text-fg'}">{p.response_ms === null ? "failed" : ms(p.response_ms)}</td>
              <td class="pr-5 text-xs">
                {#if p.cert_valid_days !== null}
                  <span class="tone-{p.cert_valid_days < 7 ? 'bad' : p.cert_valid_days < 21 ? 'warn' : 'ok'} inline-flex items-center gap-1 text-tone">
                    {#if p.cert_valid_days < 21}<ShieldAlert size={13} />{:else}<ShieldCheck size={13} />{/if}{p.cert_valid_days} days
                  </span>
                {:else}<span class="text-fg-3">—</span>{/if}
              </td>
            </tr>
          {/each}
          {#each unix as u, i (i)}
            <tr>
              <td class="num pl-5 text-fg">{u.path}</td>
              <td><span class="num text-xs text-fg-2">{u.protocol}/unix</span></td>
              <td class="text-fg-3">—</td>
              <td class="num text-xs {u.response_ms === null ? 'text-bad' : 'text-fg'}">{u.response_ms === null ? "failed" : ms(u.response_ms)}</td>
              <td class="pr-5 text-fg-3">—</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
{/snippet}

{#snippet perms()}
  <div class="flex flex-wrap gap-x-6 gap-y-2 text-xs">
    <span class="flex items-center gap-1.5 text-fg-3"><Lock size={12} /> mode <span class="num text-fg">{octalMode(d.mode)}</span></span>
    <span class="text-fg-3">uid <span class="num text-fg">{d.uid ?? "—"}</span></span>
    <span class="text-fg-3">gid <span class="num text-fg">{d.gid ?? "—"}</span></span>
    {#if d.hardlinks !== undefined && d.hardlinks !== null}<span class="text-fg-3">links <span class="num text-fg">{d.hardlinks}</span></span>{/if}
  </div>
{/snippet}

{#if service.type === "system"}
  <div class="grid gap-4 lg:grid-cols-[auto_minmax(0,1fr)]">
    <section class="card flex items-center justify-around gap-6 p-5">
      <Gauge value={d.cpu?.total ?? null} label="cpu" size={92} stroke={8} />
      <Gauge value={d.memory?.percent ?? null} label="mem" size={92} stroke={8} />
      <Gauge value={d.swap?.percent ?? null} label="swap" size={92} stroke={8} />
    </section>
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
      {@render stat("Load 1m", d.load ? d.load[0].toFixed(2) : "—", d.load ? `5m ${d.load[1].toFixed(2)} · 15m ${d.load[2].toFixed(2)}` : "")}
      {@render stat("Memory", kb(d.memory?.kb), pct(d.memory?.percent))}
      {@render stat("Swap", kb(d.swap?.kb), pct(d.swap?.percent))}
      {@render stat("Uptime", duration(d.uptime), d.boottime ? `booted ${datetime(d.boottime)}` : "")}
      {@render stat("CPU user", pct(d.cpu?.user))}
      {@render stat("CPU system", pct(d.cpu?.system))}
      {@render stat("I/O wait", pct(d.cpu?.wait))}
      {@render stat("File handles", compact(d.fd?.allocated), d.fd?.maximum ? `max ${compact(d.fd.maximum)}` : "")}
    </div>
  </div>
{:else if service.type === "process"}
  {#if d.pid === undefined || d.pid === null}
    <div class="card tone-bad p-5 text-[13px] text-fg-2"><span class="font-semibold text-tone">Process not running.</span> Monit could not find the process — no runtime data available.</div>
  {:else}
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-4 xl:grid-cols-6">
      {@render stat("PID", String(d.pid), `parent ${d.ppid ?? "—"}`)}
      {@render stat("Uptime", duration(d.uptime))}
      {@render stat("CPU", pct(d.cpu?.percent), `with children ${pct(d.cpu?.percent_total)}`)}
      {@render stat("Memory", kb(d.memory?.kb), `${pct(d.memory?.percent)} · total ${kb(d.memory?.kb_total)}`)}
      {@render stat("Threads", num(d.threads), `${num(d.children)} children`)}
      {@render stat("Open files", num(d.fd?.open), d.fd?.soft ? `limit ${compact(d.fd.soft)}` : "")}
      {@render stat("Disk read", rate(d.io?.read_bps), d.io?.read_ops !== undefined ? `${num(d.io.read_ops)} ops/s` : "")}
      {@render stat("Disk write", rate(d.io?.write_bps), d.io?.write_ops !== undefined ? `${num(d.io.write_ops)} ops/s` : "")}
      {@render stat("UID", String(d.uid ?? "—"), `euid ${d.euid ?? "—"}`)}
      {@render stat("GID", String(d.gid ?? "—"))}
    </div>
  {/if}
  {@render ports(d.ports ?? [], d.unix ?? [])}
{:else if service.type === "filesystem"}
  <div class="grid gap-4 lg:grid-cols-[auto_minmax(0,1fr)]">
    <section class="card flex items-center justify-around gap-8 p-5">
      <Gauge value={d.space?.percent ?? null} label="space" size={104} stroke={9} />
      {#if d.inodes}<Gauge value={d.inodes.percent} label="inodes" size={104} stroke={9} />{/if}
    </section>
    <section class="card space-y-4 p-5">
      <div>
        <div class="mb-1.5 flex justify-between text-xs"><span class="text-fg-2">Space</span><span class="num text-fg">{mb(d.space?.used_mb)} of {mb(d.space?.total_mb)}</span></div>
        <UsageBar value={d.space?.percent} height={10} />
        <div class="num mt-1 text-[11px] text-fg-3">{mb((d.space?.total_mb ?? 0) - (d.space?.used_mb ?? 0))} free</div>
      </div>
      {#if d.inodes}
        <div>
          <div class="mb-1.5 flex justify-between text-xs"><span class="text-fg-2">Inodes</span><span class="num text-fg">{compact(d.inodes.used)} of {compact(d.inodes.total)}</span></div>
          <UsageBar value={d.inodes.percent} height={6} tone="info" />
        </div>
      {/if}
      <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {@render stat("Type", d.fstype ?? "—")}
        {@render stat("Read", rate(d.io?.read_bps), d.io?.read_ops !== undefined ? `${num(d.io.read_ops, 1)} ops/s` : "")}
        {@render stat("Write", rate(d.io?.write_bps), d.io?.write_ops !== undefined ? `${num(d.io.write_ops, 1)} ops/s` : "")}
        {@render stat("Service time", d.servicetime ? `${num(d.servicetime.run, 2)} ms` : "—", d.servicetime ? `r ${num(d.servicetime.read, 2)} · w ${num(d.servicetime.write, 2)}` : "")}
      </div>
      {@render perms()}
      {#if d.flags}
        <div class="flex flex-wrap gap-1.5">
          {#each String(d.flags).split(",") as f (f)}<span class="num rounded-md border border-line bg-surface-2/50 px-1.5 py-0.5 text-[11px] text-fg-2">{f}</span>{/each}
        </div>
      {/if}
    </section>
  </div>
{:else if service.type === "net"}
  <div class="grid gap-4 lg:grid-cols-3">
    <section class="card p-5">
      <div class="eyebrow">Link</div>
      <div class="mt-2 flex items-center gap-2">
        <span class="tone-{d.link?.state === 1 ? 'ok' : d.link?.state === 0 ? 'bad' : 'muted'} h-2.5 w-2.5 rounded-full bg-tone glow-tone"></span>
        <span class="text-xl font-semibold text-fg">{d.link?.state === 1 ? "Up" : d.link?.state === 0 ? "Down" : "n/a"}</span>
      </div>
      <div class="num mt-3 space-y-1 text-xs text-fg-2">
        <div>Speed <span class="text-fg">{bits(d.link?.speed)}</span></div>
        <div>Duplex <span class="text-fg">{d.link?.duplex === 1 ? "full" : d.link?.duplex === 0 ? "half" : "n/a"}</span></div>
      </div>
    </section>
    {#each [{ key: "download", label: "Download", Icon: ArrowDown }, { key: "upload", label: "Upload", Icon: ArrowUp }] as { key, label, Icon } (key)}
      {@const x = d[key] ?? {}}
      <section class="card p-5">
        <div class="flex items-center gap-1.5 eyebrow"><Icon size={12} />{label}</div>
        <div class="num mt-2 text-2xl font-semibold text-fg">{rate(x.bytes)}</div>
        <div class="num mt-3 grid grid-cols-2 gap-2 text-xs text-fg-2">
          <div>{compact(x.packets)} pkt/s</div>
          <div class={x.errors ? "text-bad" : ""}>{num(x.errors)} err/s</div>
          <div class="text-fg-3">total {bytes(x.bytes_total)}</div>
          <div class="text-fg-3">{compact(x.packets_total)} pkts</div>
        </div>
      </section>
    {/each}
  </div>
{:else if service.type === "host"}
  {#if d.icmp?.length}
    <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
      {#each d.icmp as i, k (k)}
        {@render stat(`ICMP ${i.type}`, i.response_ms === null ? "failed" : ms(i.response_ms))}
      {/each}
    </div>
  {/if}
  {@render ports(d.ports ?? [], d.unix ?? [])}
  {#if !d.icmp?.length && !d.ports?.length && !d.unix?.length}
    <div class="card p-5 text-[13px] text-fg-3">No connection tests reported.</div>
  {/if}
{:else if service.type === "program"}
  <div class="grid gap-4 lg:grid-cols-[260px_minmax(0,1fr)]">
    <div class="grid grid-cols-2 gap-3 lg:grid-cols-1">
      <div class="card tone-{d.exit_status === 0 ? 'ok' : d.exit_status === undefined || d.exit_status === null ? 'muted' : 'bad'} p-5">
        <div class="eyebrow">Exit status</div>
        <div class="num mt-1 text-4xl font-semibold text-tone">{d.exit_status ?? "—"}</div>
      </div>
      {@render stat("Last run", d.started ? ago(d.started, clock.now) : "—", d.started ? datetime(d.started) : "")}
    </div>
    <section class="card overflow-hidden">
      <div class="flex items-center gap-2 border-b border-line px-5 py-3">
        <SquareTerminal size={14} class="text-fg-3" />
        <h3 class="card-title">Output</h3>
      </div>
      <pre class="num max-h-[420px] min-h-28 overflow-auto bg-[var(--glass)] p-5 text-[12.5px] leading-relaxed whitespace-pre-wrap text-fg">{d.output || "(no output)"}</pre>
    </section>
  </div>
{:else if service.type === "file" || service.type === "directory" || service.type === "fifo"}
  <div class="grid gap-4 lg:grid-cols-2">
    <section class="card space-y-4 p-5">
      {#if service.type === "file"}
        <div class="grid grid-cols-2 gap-3">
          {@render stat("Size", bytes(d.size), d.size !== undefined && d.size !== null ? `${num(d.size)} bytes` : "")}
          {@render stat("Hard links", num(d.hardlinks))}
        </div>
      {/if}
      {@render perms()}
      {#if d.checksum}
        <div>
          <div class="eyebrow">{d.checksum.type} checksum</div>
          <div class="num mt-1 rounded-lg border border-line bg-surface-2/50 px-3 py-2 text-xs break-all text-fg-2">{d.checksum.value}</div>
        </div>
      {/if}
    </section>
    <section class="card p-5">
      <div class="mb-3 eyebrow">Timestamps</div>
      <dl class="space-y-3 text-[13px]">
        {#each [["Modified", d.timestamps?.modify], ["Changed", d.timestamps?.change], ["Accessed", d.timestamps?.access]] as [label, ts] (label)}
          <div class="flex items-center justify-between gap-4">
            <dt class="text-fg-3">{label}</dt>
            <dd class="num text-right text-fg">{ts ? datetime(ts as number) : "—"} <span class="text-[11px] text-fg-3">{ts ? `(${ago(ts as number, clock.now)})` : ""}</span></dd>
          </div>
        {/each}
      </dl>
    </section>
  </div>
{/if}
