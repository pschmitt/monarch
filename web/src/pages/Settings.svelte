<script lang="ts">
  import { Bell, Info, KeyRound, LockKeyhole, Palette, Radio, Server, SlidersHorizontal, UserRound, Users } from "@lucide/svelte";
  import { can, session } from "../lib/state.svelte";
  import General from "./settings/General.svelte";
  import Collector from "./settings/Collector.svelte";
  import Hosts from "./settings/Hosts.svelte";
  import UsersSection from "./settings/Users.svelte";
  import Notifications from "./settings/Notifications.svelte";
  import About from "./settings/About.svelte";
  import Profile from "./settings/Profile.svelte";
  import Password from "./settings/Password.svelte";
  import Tokens from "./settings/Tokens.svelte";
  import Appearance from "./settings/Appearance.svelte";

  let { section: requested }: { section?: string } = $props();

  const admin = $derived(can("admin"));
  // Accounts signed in through single sign-on have no local password.
  const localPassword = $derived(session.me?.user?.auth_source !== "oidc");

  // Account sections are for everyone; the server-wide ones only for admins.
  const groups = $derived([
    {
      label: "Account",
      sections: [
        { id: "profile", label: "Profile", icon: UserRound, desc: "Name, email & picture" },
        ...(localPassword ? [{ id: "password", label: "Password", icon: LockKeyhole, desc: "Change your password" }] : []),
        { id: "tokens", label: "API tokens", icon: KeyRound, desc: "Access the API as you" },
        { id: "appearance", label: "Appearance", icon: Palette, desc: "Theme, mode & favicon" },
      ],
    },
    ...(admin
      ? [
          {
            label: "Administration",
            sections: [
              { id: "general", label: "General", icon: SlidersHorizontal, desc: "URL, retention, heartbeats" },
              { id: "collector", label: "Collector", icon: Radio, desc: "Agents push to Monarch" },
              { id: "hosts", label: "Hosts", icon: Server, desc: "Action URLs & pull connections" },
              { id: "users", label: "Users", icon: Users, desc: "Accounts & roles" },
              { id: "notifications", label: "Notifications", icon: Bell, desc: "Channels & routing" },
            ],
          },
        ]
      : []),
    { label: "Monarch", sections: [{ id: "about", label: "About", icon: Info, desc: "Version & license" }] },
  ]);

  // Settings › Connections was folded into Settings › Hosts. Unknown or
  // forbidden sections fall back to the first one the user may open.
  const section = $derived.by(() => {
    const id = requested === "connections" ? "hosts" : (requested ?? (admin ? "general" : "profile"));
    return groups.some((g) => g.sections.some((s) => s.id === id)) ? id : "profile";
  });
</script>

<div class="space-y-6">
  <div>
    <h1 class="text-2xl font-semibold tracking-tight text-fg">Settings</h1>
    <p class="mt-1 text-[13px] text-fg-3">Your account{admin ? " and how Monarch is configured" : ""}. No license key required — ever.</p>
  </div>

  <div class="grid gap-6 lg:grid-cols-[240px_minmax(0,1fr)]">
    <nav class="flex gap-1 overflow-x-auto lg:flex-col" aria-label="Settings sections">
      {#each groups as g (g.label)}
        <div class="eyebrow hidden px-3 pt-3 pb-1 first:pt-0 lg:block">{g.label}</div>
        {#each g.sections as s (s.id)}
          <a
            href="/settings/{s.id}"
            class="flex shrink-0 items-center gap-3 rounded-xl px-3 py-2.5 transition-colors {section === s.id ? 'bg-hover text-fg ring-1 ring-[var(--mn-line)]' : 'text-fg-2 hover:bg-hover hover:text-fg'}"
            aria-current={section === s.id ? "page" : undefined}
          >
            <s.icon size={16} class={section === s.id ? "text-accent" : "text-fg-3"} />
            <span>
              <span class="block text-[13px] font-medium">{s.label}</span>
              <span class="hidden text-[11px] text-fg-3 lg:block">{s.desc}</span>
            </span>
          </a>
        {/each}
      {/each}
    </nav>
    <div class="min-w-0">
      {#if section === "profile"}<Profile />
      {:else if section === "password"}<Password />
      {:else if section === "tokens"}<Tokens />
      {:else if section === "appearance"}<Appearance />
      {:else if section === "collector"}<Collector />
      {:else if section === "hosts"}<Hosts />
      {:else if section === "users"}<UsersSection />
      {:else if section === "notifications"}<Notifications />
      {:else if section === "about"}<About />
      {:else}<General />{/if}
    </div>
  </div>
</div>
