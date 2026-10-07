<script lang="ts">
  import { api, onUnauthorized } from "./lib/api";
  import { router } from "./lib/router.svelte";
  import { refreshSession, session, startLive, stopLive, toastError } from "./lib/state.svelte";
  import AppShell from "./lib/components/AppShell.svelte";
  import CommandPalette from "./lib/components/CommandPalette.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import Logo from "./lib/components/Logo.svelte";
  import Login from "./pages/Login.svelte";
  import Overview from "./pages/Overview.svelte";
  import Hosts from "./pages/Hosts.svelte";
  import Host from "./pages/Host.svelte";
  import Service from "./pages/Service.svelte";
  import Events from "./pages/Events.svelte";
  import Settings from "./pages/Settings.svelte";
  import NotFound from "./pages/NotFound.svelte";

  refreshSession();
  onUnauthorized(() => {
    if (session.me) session.me = { ...session.me, user: null };
  });

  const authed = $derived(!!session.me?.user);

  $effect(() => {
    if (authed) startLive();
    else stopLive();
  });

  // Release the SSE connection when the page is hidden (navigation, reload,
  // back/forward cache). Browsers allow only ~6 HTTP/1.1 connections per
  // host, and lingering streams would otherwise starve new page loads.
  $effect(() => {
    const hide = () => stopLive();
    const show = (e: PageTransitionEvent) => {
      if (e.persisted && authed) startLive();
    };
    window.addEventListener("pagehide", hide);
    window.addEventListener("pageshow", show);
    return () => {
      window.removeEventListener("pagehide", hide);
      window.removeEventListener("pageshow", show);
    };
  });

  // Keep the URL in sync with the auth state.
  $effect(() => {
    if (session.loading) return;
    const name = router.route.name;
    if (session.me?.setup_required) {
      if (name !== "setup") router.go("/setup", true);
    } else if (!authed) {
      if (name !== "login") router.go(`/login${name !== "overview" && name !== "setup" ? `?next=${encodeURIComponent(router.route.path)}` : ""}`, true);
    } else if (name === "login" || name === "setup") {
      router.go(router.route.query.get("next") || "/", true);
    }
  });

  async function logout() {
    try {
      await api.logout();
    } catch (e) {
      toastError(e);
    }
    session.me = session.me ? { ...session.me, user: null } : null;
  }

  const titles: Record<string, string> = {
    overview: "Overview",
    hosts: "Hosts",
    events: "Events",
    settings: "Settings",
    login: "Sign in",
    setup: "Welcome",
  };
  $effect(() => {
    const t = titles[router.route.name];
    if (t) document.title = `${t} · Monarch`;
  });
</script>

{#if session.loading}
  <div class="flex min-h-dvh items-center justify-center">
    <div class="animate-pulse-soft"><Logo size={44} /></div>
  </div>
{:else if !authed}
  <Login setup={!!session.me?.setup_required} />
{:else}
  <AppShell onlogout={logout}>
    {#key router.route.name === "host" || router.route.name === "service" ? router.route.path.split("/").slice(0, 3).join("/") + router.route.name : router.route.name}
      <div class="animate-in">
        {#if router.route.name === "overview"}
          <Overview />
        {:else if router.route.name === "hosts"}
          <Hosts />
        {:else if router.route.name === "host"}
          <Host id={+router.route.params.id} tab={router.route.params.tab ?? "overview"} />
        {:else if router.route.name === "service"}
          <Service hostId={+router.route.params.id} name={router.route.params.name} />
        {:else if router.route.name === "events"}
          <Events />
        {:else if router.route.name === "settings"}
          <Settings section={router.route.params.section ?? "general"} />
        {:else if router.route.name === "login" || router.route.name === "setup"}
          <div></div>
        {:else}
          <NotFound />
        {/if}
      </div>
    {/key}
  </AppShell>
  <CommandPalette onlogout={logout} />
{/if}

<ConfirmDialog />
<Toasts />
