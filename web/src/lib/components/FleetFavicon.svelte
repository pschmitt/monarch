<script lang="ts">
  import crown from "../../../public/favicon.svg?raw";
  import { faviconPreference } from "../favicon.svelte";
  import { fleet } from "../state.svelte";

  let { authed }: { authed: boolean } = $props();

  // An unreachable host is one issue; its stale service failures aren't counted again.
  const issues = $derived(Object.values(fleet.hosts).reduce(
    (total, host) => total + (!host.online || host.state === "offline" ? 1 : Math.max(host.services.failed, host.state === "degraded" ? 1 : 0)), 0,
  ));
  const unknown = $derived(!fleet.loaded || fleet.loadError || fleet.status !== "live");
  const label = $derived(unknown ? "Fleet status unavailable" : issues ? `${issues} fleet issue${issues === 1 ? "" : "s"}` : "Fleet healthy");
  const href = $derived.by(() => {
    if (!authed) return "/favicon.svg";
    const text = unknown ? "!" : issues && faviconPreference.mode === "count" ? (issues > 99 ? "99+" : String(issues)) : "";
    const color = unknown ? "#f59e0b" : issues ? "#dc2626" : "#16a34a";
    const width = text.length > 2 ? 12 : text.length > 1 ? 10 : 8;
    const badge = text
      ? `<rect x="${16 - width}" y="8" width="${width}" height="8" rx="4" fill="${color}" stroke="#fff" stroke-width=".6"/><text x="${16 - width / 2}" y="12.3" text-anchor="middle" dominant-baseline="middle" font-family="Arial,sans-serif" font-size="${text.length > 2 ? 5 : 6}" font-weight="700" fill="#fff">${text}</text>`
      : `<circle cx="12.5" cy="12.5" r="3" fill="${color}" stroke="#fff" stroke-width=".8"/>`;
    return `data:image/svg+xml,${encodeURIComponent(crown.replace("</svg>", `${badge}</svg>`))}`;
  });

  $effect(() => {
    // Update both declarations so the ICO fallback cannot win over the live badge.
    const icons = document.querySelectorAll<HTMLLinkElement>('link[rel="icon"]');
    for (const icon of icons) {
      icon.href = href;
      icon.type = "image/svg+xml";
      icon.removeAttribute("sizes");
      icon.title = authed ? label : "Monarch";
    }
  });
</script>
