<script lang="ts">
  import { Check, Moon, Sun } from "@lucide/svelte";
  import { faviconPreference, setFaviconCountSource, setFaviconMode, type FaviconCountSource, type FaviconMode } from "../../lib/favicon.svelte";
  import { setTheme, setThemeStyle, theme, themeStyles, type Theme } from "../../lib/state.svelte";

  const styleDesc: Record<string, string> = {
    monarch: "Glass surfaces, gradients and glows.",
    simple: "Flat and opaque, inspired by nixbot.",
    character: "A brighter lilac and apricot look, with a spacious navigation shell.",
  };
  const modes: { id: Theme; label: string; icon: typeof Sun }[] = [
    { id: "dark", label: "Dark", icon: Moon },
    { id: "light", label: "Light", icon: Sun },
  ];
  const faviconModes: { id: FaviconMode; label: string }[] = [
    { id: "count", label: "Issue count" },
    { id: "binary", label: "Status dot" },
  ];
  const faviconCountSources: { id: FaviconCountSource; label: string; description: string }[] = [
    { id: "issues", label: "Fleet issues", description: "Counts failed services and offline hosts, matching the current count behavior." },
    { id: "hosts", label: "Hosts", description: "Counts distinct unhealthy hosts. Offline hosts are included." },
    { id: "services", label: "Services", description: "Counts active failures on online hosts. Offline hosts and their stale service states still flag the icon but are excluded from this count." },
  ];
</script>

<div class="space-y-6">
  <section class="card space-y-4 p-5 sm:p-6">
    <div>
      <h2 class="card-title">Theme</h2>
      <p class="mt-1 text-xs text-fg-3">Appearance settings are stored in this browser only.</p>
    </div>
    <div class="grid gap-3 sm:grid-cols-2" role="radiogroup" aria-label="Theme">
      {#each themeStyles as st (st.id)}
        {@const active = theme.style === st.id}
        <button
          type="button"
          role="radio"
          aria-checked={active}
          class="flex items-start gap-3 rounded-xl border p-4 text-left transition-colors {active ? 'border-accent bg-hover' : 'border-line hover:border-line-strong'}"
          onclick={() => setThemeStyle(st.id)}
        >
          <span class="flex-1">
            <span class="block text-[13px] font-medium text-fg">{st.label}</span>
            <span class="mt-0.5 block text-xs text-fg-3">{styleDesc[st.id]}</span>
          </span>
          {#if active}<Check size={16} class="text-accent" />{/if}
        </button>
      {/each}
    </div>
  </section>

  <section class="card space-y-4 p-5 sm:p-6">
    <div>
      <h2 class="card-title">Mode</h2>
      <p class="mt-1 text-xs text-fg-3">Also available from the sun/moon button in the header.</p>
    </div>
    <div class="flex gap-2" role="radiogroup" aria-label="Mode">
      {#each modes as m (m.id)}
        <button type="button" role="radio" aria-checked={theme.value === m.id} class="btn {theme.value === m.id ? 'btn-primary' : ''}" onclick={() => setTheme(m.id)}>
          <m.icon size={15} />
          {m.label}
        </button>
      {/each}
    </div>
  </section>

  <section class="card space-y-4 p-5 sm:p-6">
    <div>
      <h2 class="card-title">Favicon</h2>
      <p class="mt-1 text-xs text-fg-3">How the browser tab shows fleet problems: the number of active failures, or just a status dot.</p>
    </div>
    <div class="flex gap-2" role="radiogroup" aria-label="Favicon">
      {#each faviconModes as f (f.id)}
        <button type="button" role="radio" aria-checked={faviconPreference.mode === f.id} class="btn {faviconPreference.mode === f.id ? 'btn-primary' : ''}" onclick={() => setFaviconMode(f.id)}>
          {f.label}
        </button>
      {/each}
    </div>
    {#if faviconPreference.mode === "count"}
      <div>
        <h3 class="text-[13px] font-medium text-fg">Count</h3>
        <p class="mt-1 text-xs text-fg-3">Choose what the number in the browser tab counts.</p>
        <div class="mt-3 grid gap-3 sm:grid-cols-3" role="radiogroup" aria-label="Favicon count source">
          {#each faviconCountSources as source (source.id)}
            {@const active = faviconPreference.countSource === source.id}
            <button
              type="button"
              role="radio"
              aria-checked={active}
              class="flex items-start gap-3 rounded-xl border p-4 text-left transition-colors {active ? 'border-accent bg-hover' : 'border-line hover:border-line-strong'}"
              onclick={() => setFaviconCountSource(source.id)}
            >
              <span class="flex-1">
                <span class="block text-[13px] font-medium text-fg">{source.label}</span>
                <span class="mt-0.5 block text-xs text-fg-3">{source.description}</span>
              </span>
              {#if active}<Check size={16} class="text-accent" />{/if}
            </button>
          {/each}
        </div>
      </div>
    {/if}
  </section>
</div>
