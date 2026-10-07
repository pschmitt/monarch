<script lang="ts">
  import { ArrowDownToLine, ArrowLeft, ArrowUpFromLine, LoaderCircle } from "@lucide/svelte";
  import { api } from "../api";
  import type { Target } from "../types";
  import { router, hostHref } from "../router.svelte";
  import { can, toast, ui } from "../state.svelte";
  import CollectorSnippet from "./CollectorSnippet.svelte";
  import Modal from "./Modal.svelte";
  import TargetForm from "./TargetForm.svelte";

  let mode = $state<"choose" | "push" | "pull">("choose");
  let collectorUrl = $state(`${location.origin}/collector`);
  let waiting = $state<string | null>(null);

  $effect(() => {
    if (!ui.addHost) return;
    mode = "choose";
    waiting = null;
    if (can("admin")) {
      api
        .settings()
        .then((s) => (collectorUrl = s.collector_url))
        .catch(() => (collectorUrl = `${location.origin}/collector`));
    }
  });

  async function created(t: Target) {
    waiting = t.name;
    let hostId = t.host_id;
    for (let i = 0; i < 6 && !hostId; i++) {
      await new Promise((r) => setTimeout(r, 1000));
      try {
        hostId = (await api.targets()).find((x) => x.id === t.id)?.host_id ?? null;
      } catch {
        break;
      }
    }
    ui.addHost = false;
    if (hostId) {
      toast("ok", `${t.name} connected`, "Monarch is now polling this agent.");
      router.go(hostHref(hostId));
    } else {
      toast("info", `${t.name} added`, "Waiting for the first successful poll.");
      router.go("/settings/connections");
    }
  }
</script>

<Modal bind:open={ui.addHost} title={mode === "push" ? "Agent pushes to Monarch" : mode === "pull" ? "Monarch pulls from agent" : "Add a host"} width="max-w-2xl">
  {#if waiting}
    <div class="flex flex-col items-center gap-3 py-12 text-center animate-in">
      <LoaderCircle size={28} class="animate-spin text-accent" />
      <div class="text-sm font-medium text-fg">Polling {waiting} for the first time…</div>
      <div class="text-xs text-fg-3">This usually takes a few seconds.</div>
    </div>
  {:else if mode === "choose"}
    <p class="mb-4 text-[13px] text-fg-2">How should Monarch get data from this Monit agent?</p>
    <div class="grid gap-3 sm:grid-cols-2">
      <button class="choice group" onclick={() => (mode = "push")}>
        <span class="choice-icon bg-accent-gradient text-white"><ArrowUpFromLine size={20} /></span>
        <span class="mt-4 block text-[15px] font-semibold text-fg">Agent pushes to Monarch</span>
        <span class="mt-1.5 block text-xs leading-relaxed text-fg-3">Add a <code class="num">set mmonit</code> line to monitrc. Best when agents can reach Monarch. Real-time events.</span>
        <span class="mt-3 inline-block rounded-md border border-line px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-fg-3 uppercase">M/Monit compatible</span>
      </button>
      <button class="choice group" onclick={() => (mode = "pull")} disabled={!can("admin")}>
        <span class="choice-icon border border-line-strong bg-surface-2 text-accent-2"><ArrowDownToLine size={20} /></span>
        <span class="mt-4 block text-[15px] font-semibold text-fg">Monarch pulls from agent</span>
        <span class="mt-1.5 block text-xs leading-relaxed text-fg-3">Monarch polls Monit's HTTP interface — directly or tunnelled over SSH. No agent config changes needed.</span>
        <span class="mt-3 inline-block rounded-md border border-line px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-fg-3 uppercase">{can("admin") ? "Works behind NAT via SSH" : "Admins only"}</span>
      </button>
    </div>
  {:else}
    <button class="mb-4 flex items-center gap-1.5 text-xs text-fg-3 hover:text-fg" onclick={() => (mode = "choose")}><ArrowLeft size={13} /> Back</button>
    {#if mode === "push"}
      <div class="space-y-4">
        <p class="text-[13px] leading-relaxed text-fg-2">
          Add this to <code class="num text-xs">monitrc</code> on the host and run <code class="num text-xs">monit reload</code>. It appears on the overview within one poll cycle.
        </p>
        <CollectorSnippet {collectorUrl} compact />
        <div class="flex justify-end gap-2 pt-1">
          {#if can("admin")}<a class="btn btn-ghost" href="/settings/collector" onclick={() => (ui.addHost = false)}>More options</a>{/if}
          <button class="btn btn-primary" onclick={() => (ui.addHost = false)}>Done</button>
        </div>
      </div>
    {:else}
      <TargetForm onsaved={created} oncancel={() => (ui.addHost = false)} />
    {/if}
  {/if}
</Modal>

<style>
  .choice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: flex-start;
    text-align: left;
    border-radius: 16px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    padding: 1.25rem;
    transition:
      border-color 160ms ease,
      transform 160ms ease,
      background-color 160ms ease;
  }
  .choice:hover:not(:disabled) {
    border-color: color-mix(in oklab, var(--accent) 55%, transparent);
    background: color-mix(in oklab, var(--accent) 7%, var(--surface-2));
    transform: translateY(-1px);
  }
  .choice:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .choice-icon {
    display: flex;
    height: 2.75rem;
    width: 2.75rem;
    align-items: center;
    justify-content: center;
    border-radius: 14px;
  }
</style>
