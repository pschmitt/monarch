<script lang="ts">
  import { ArrowDownToLine, Bell, Info, Radio, SlidersHorizontal, Users } from "@lucide/svelte";
  import { can } from "../lib/state.svelte";
  import General from "./settings/General.svelte";
  import Collector from "./settings/Collector.svelte";
  import Connections from "./settings/Connections.svelte";
  import UsersSection from "./settings/Users.svelte";
  import Notifications from "./settings/Notifications.svelte";
  import About from "./settings/About.svelte";

  let { section = "general" }: { section?: string } = $props();

  const sections = [
    { id: "general", label: "General", icon: SlidersHorizontal, desc: "URL, retention, heartbeats" },
    { id: "collector", label: "Collector", icon: Radio, desc: "Agents push to Monarch" },
    { id: "connections", label: "Connections", icon: ArrowDownToLine, desc: "Monarch pulls from agents" },
    { id: "users", label: "Users", icon: Users, desc: "Accounts & roles" },
    { id: "notifications", label: "Notifications", icon: Bell, desc: "Channels & routing" },
    { id: "about", label: "About", icon: Info, desc: "Version & license" },
  ];
</script>

<div class="space-y-6">
  <div>
    <h1 class="text-2xl font-semibold tracking-tight text-fg">Settings</h1>
    <p class="mt-1 text-[13px] text-fg-3">Configure Monarch. No license key required — ever.</p>
  </div>

  {#if !can("admin")}
    <div class="card p-6 text-sm text-fg-2">Only administrators can change settings.</div>
  {:else}
    <div class="grid gap-6 lg:grid-cols-[240px_minmax(0,1fr)]">
      <nav class="flex gap-1 overflow-x-auto lg:flex-col" aria-label="Settings sections">
        {#each sections as s (s.id)}
          <a
            href="/settings{s.id === 'general' ? '' : `/${s.id}`}"
            class="flex shrink-0 items-center gap-3 rounded-xl px-3 py-2.5 transition-colors {section === s.id ? 'bg-hover text-fg ring-1 ring-[var(--line)]' : 'text-fg-2 hover:bg-hover hover:text-fg'}"
            aria-current={section === s.id ? "page" : undefined}
          >
            <s.icon size={16} class={section === s.id ? "text-accent" : "text-fg-3"} />
            <span>
              <span class="block text-[13px] font-medium">{s.label}</span>
              <span class="hidden text-[11px] text-fg-3 lg:block">{s.desc}</span>
            </span>
          </a>
        {/each}
      </nav>
      <div class="min-w-0">
        {#if section === "collector"}<Collector />
        {:else if section === "connections"}<Connections />
        {:else if section === "users"}<UsersSection />
        {:else if section === "notifications"}<Notifications />
        {:else if section === "about"}<About />
        {:else}<General />{/if}
      </div>
    </div>
  {/if}
</div>
