<script lang="ts">
  import type { Snippet } from "svelte";
  import {
    Activity,
    ChevronsLeft,
    ChevronsRight,
    LayoutDashboard,
    LogOut,
    Menu as MenuIcon,
    Moon,
    Pencil,
    Search,
    Server,
    Settings,
    KeyRound,
    Sun,
    UserRound,
  } from "@lucide/svelte";
  import { isMock } from "../api";
  import { router } from "../router.svelte";
  import { can, fleet, session, theme, toggleSidebar, toggleTheme, ui } from "../state.svelte";
  import ChangePassword from "./ChangePassword.svelte";
  import ApiTokens from "./ApiTokens.svelte";
  import ChangeUsername from "./ChangeUsername.svelte";
  import Logo from "./Logo.svelte";
  import Menu from "./Menu.svelte";
  import StatusDot from "./StatusDot.svelte";

  let { children, onlogout }: { children: Snippet; onlogout: () => void } = $props();
  let pwOpen = $state(false);
  let nameOpen = $state(false);
  let tokensOpen = $state(false);

  const failingHosts = $derived(Object.values(fleet.hosts).filter((h) => h.state !== "ok").length);
  const unseen = $derived(fleet.live.filter((e) => e.state === "failed" && !e.acked_by).length);

  const nav = $derived([
    { href: "/", label: "Overview", icon: LayoutDashboard, active: router.route.name === "overview", badge: 0 },
    {
      href: "/hosts",
      label: "Hosts",
      icon: Server,
      active: ["hosts", "host", "service"].includes(router.route.name),
      badge: failingHosts,
    },
    { href: "/events", label: "Events", icon: Activity, active: router.route.name === "events", badge: unseen },
    ...(can("admin") ? [{ href: "/settings", label: "Settings", icon: Settings, active: router.route.name === "settings", badge: 0 }] : []),
  ]);

  const liveTone = $derived(fleet.status === "live" ? "ok" : fleet.status === "connecting" ? "warn" : "bad");
  const liveLabel = $derived(fleet.status === "live" ? "Live" : fleet.status === "connecting" ? "Connecting" : "Offline");
  const collapsed = $derived(ui.sidebarCollapsed);

  $effect(() => {
    void router.route.path;
    ui.mobileNav = false;
  });
</script>

{#snippet sidebar(mobile: boolean)}
  <div class="flex h-full flex-col">
    <div class="flex h-16 items-center px-4 {collapsed && !mobile ? 'justify-center px-0' : ''}">
      <a href="/" class="flex items-center" aria-label="Monarch home"><Logo size={30} wordmark={!collapsed || mobile} /></a>
    </div>

    <nav class="mt-2 flex-1 space-y-1 px-3" aria-label="Main">
      {#each nav as item (item.href)}
        <a
          href={item.href}
          title={collapsed && !mobile ? item.label : undefined}
          class="group relative flex h-10 items-center gap-3 rounded-xl px-3 text-[13px] font-medium transition-all {item.active
            ? 'bg-hover text-fg ring-1 ring-[var(--line)]'
            : 'text-fg-2 hover:bg-hover hover:text-fg'} {collapsed && !mobile ? 'justify-center px-0' : ''}"
          aria-current={item.active ? "page" : undefined}
        >
          {#if item.active}
            <span class="absolute top-2 bottom-2 left-0 w-[3px] rounded-r-full bg-accent-gradient"></span>
          {/if}
          <item.icon size={18} strokeWidth={item.active ? 2 : 1.75} class={item.active ? "text-accent" : ""} />
          {#if !collapsed || mobile}<span class="flex-1">{item.label}</span>{/if}
          {#if item.badge > 0}
            <span
              class="tone-bad num flex h-5 min-w-5 items-center justify-center rounded-full bg-tone px-1.5 text-[10px] font-bold text-white {collapsed && !mobile
                ? 'absolute -top-0.5 -right-0.5 h-4 min-w-4 px-1 text-[9px]'
                : ''}">{item.badge}</span
            >
          {/if}
        </a>
      {/each}
    </nav>

    <div class="space-y-2 p-3">
      {#if !collapsed || mobile}
        <div class="rounded-xl border border-line bg-surface-2/60 p-3">
          <div class="flex items-center gap-2 text-xs">
            <StatusDot tone={liveTone} pulse={fleet.status === "live"} />
            <span class="font-medium text-fg">{liveLabel}</span>
            <span class="ml-auto num text-fg-3">{Object.keys(fleet.hosts).length} hosts</span>
          </div>
          <div class="mt-1.5 text-[11px] leading-snug text-fg-3">
            {#if isMock}Demo data — mock backend{:else}Streaming updates from your Monit agents{/if}
          </div>
        </div>
      {/if}
      {#if !mobile}
        <button class="btn btn-ghost w-full {collapsed ? 'btn-icon' : 'justify-start'}" onclick={toggleSidebar} aria-label="Toggle sidebar">
          {#if collapsed}<ChevronsRight size={16} />{:else}<ChevronsLeft size={16} /> <span class="text-xs">Collapse</span>{/if}
        </button>
      {/if}
    </div>
  </div>
{/snippet}

<div class="flex min-h-dvh">
  <!-- desktop sidebar -->
  <aside
    class="sticky top-0 hidden h-dvh shrink-0 border-r border-line bg-[var(--glass)] backdrop-blur-xl transition-[width] duration-200 md:block {collapsed
      ? 'w-[72px]'
      : 'w-60'}"
  >
    {@render sidebar(false)}
  </aside>

  <!-- mobile drawer -->
  {#if ui.mobileNav}
    <div class="fixed inset-0 z-50 md:hidden">
      <button class="absolute inset-0 bg-[var(--scrim)] backdrop-blur-sm" aria-label="Close navigation" onclick={() => (ui.mobileNav = false)}></button>
      <aside class="absolute inset-y-0 left-0 w-72 border-r border-line bg-surface-solid shadow-2xl" style="animation: fade-up 180ms both">
        {@render sidebar(true)}
      </aside>
    </div>
  {/if}

  <div class="flex min-w-0 flex-1 flex-col">
    <header class="sticky top-0 z-30 border-b border-line bg-[var(--glass)] backdrop-blur-xl">
      <div class="flex h-16 items-center gap-3 px-4 sm:px-6">
        <button class="btn btn-ghost btn-icon md:hidden" aria-label="Open navigation" onclick={() => (ui.mobileNav = true)}><MenuIcon size={18} /></button>
        <div class="md:hidden"><Logo size={26} /></div>

        <button
          class="group flex h-10 max-w-md flex-1 items-center gap-2.5 rounded-xl border border-line bg-surface-2/70 px-3 text-left text-[13px] text-fg-3 transition-colors hover:border-line-strong hover:text-fg-2"
          onclick={() => (ui.palette = true)}
        >
          <Search size={16} />
          <span class="flex-1 truncate">Search hosts, services…</span>
          <span class="kbd hidden sm:inline-flex">⌘K</span>
        </button>

        <div class="ml-auto flex items-center gap-1.5">
          <div class="mr-1 hidden items-center gap-2 rounded-full border border-line px-2.5 py-1 text-[11px] text-fg-2 sm:flex" title="Live connection">
            <StatusDot tone={liveTone} pulse={fleet.status === "live"} size={7} />
            {liveLabel}
          </div>
          <button class="btn btn-ghost btn-icon" onclick={toggleTheme} aria-label="Toggle theme">
            {#if theme.value === "dark"}<Sun size={17} />{:else}<Moon size={17} />{/if}
          </button>
          <Menu
            label="User menu"
            items={[
              { label: `Signed in as ${session.me?.user?.username ?? "?"}${session.me?.user?.auth_source === "oidc" ? " (SSO)" : ""}`, icon: UserRound, disabled: true, onselect: () => {} },
              ...(session.me?.user?.auth_source === "oidc"
                ? []
                : [
                    { label: "Change username", icon: Pencil, onselect: () => (nameOpen = true) },
                    { label: "Change password", icon: KeyRound, onselect: () => (pwOpen = true) },
                  ]),
              { label: "API tokens", icon: KeyRound, onselect: () => (tokensOpen = true) },
              ...(can("admin") ? [{ label: "Settings", icon: Settings, onselect: () => router.go("/settings") }] : []),
              "sep" as const,
              { label: "Sign out", icon: LogOut, danger: true, onselect: onlogout },
            ]}
          >
            {#snippet trigger()}
              <span class="bg-accent-gradient flex h-7 w-7 items-center justify-center rounded-full text-[11px] font-bold text-white uppercase">
                {session.me?.user?.username?.slice(0, 2) ?? "?"}
              </span>
            {/snippet}
          </Menu>
        </div>
      </div>
    </header>

    <main class="mx-auto w-full max-w-[1500px] flex-1 px-4 py-6 sm:px-6 lg:px-8 lg:py-8">
      {@render children()}
    </main>
  </div>
</div>

<ChangePassword bind:open={pwOpen} />
<ChangeUsername bind:open={nameOpen} />
<ApiTokens bind:open={tokensOpen} />
