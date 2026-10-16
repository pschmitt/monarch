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
    // Element-style notification bubble: circle for one digit, pill for longer counts.
    // Keep the badge at roughly 60% of the icon height so the crown stays visible.
    const width = text.length > 2 ? 15 : text.length > 1 ? 14 : 10;
    const fontSize = text.length > 2 ? 8 : 10;
    const x = 15.5 - width;
    const badge = text
      ? `<rect x="${x}" y="5.5" width="${width}" height="10" rx="5" fill="${color}" stroke="#fff" stroke-width="1"/><text x="${x + width / 2}" y="10.8" text-anchor="middle" dominant-baseline="central" font-family="Arial,sans-serif" font-size="${fontSize}" font-weight="700" fill="#fff">${text}</text>`
      : `<circle cx="12.5" cy="12.5" r="3" fill="${color}" stroke="#fff" stroke-width="1"/>`;
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
