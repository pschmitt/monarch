<script lang="ts">
  import { onMount } from "svelte";
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
  let baseImage = $state<HTMLImageElement | null>(null);
  onMount(() => {
    const image = new Image();
    image.onload = () => (baseImage = image);
    image.src = `data:image/svg+xml,${encodeURIComponent(crown)}`;
    return () => { image.onload = null; };
  });

  const href = $derived.by(() => {
    if (!authed) return "/favicon.svg";
    const text = unknown ? "!" : issues && faviconPreference.mode === "count" ? (issues > 99 ? "99+" : String(issues)) : "";
    if (!baseImage) return "/favicon.svg";
    // Match Element's high-resolution canvas, badge proportions and optical text centering.
    const size = 144;
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = size;
    const ctx = canvas.getContext("2d");
    if (!ctx) return "/favicon.svg";
    ctx.drawImage(baseImage, 0, 0, size, size);
    const color = unknown ? "#f59e0b" : issues ? "#d00" : "#16a34a";
    ctx.fillStyle = color;
    if (text) {
      const height = size * 0.6;
      const width = height * (text.length > 2 ? 1.65 : text.length > 1 ? 1.4 : 1);
      const x = size - width;
      const y = size - height;
      ctx.beginPath();
      ctx.roundRect(x, y, width, height, height / 2);
      ctx.fill();
      const fontSize = Math.floor(height * (text.length > 2 ? 0.85 : 1));
      ctx.font = `bold ${fontSize}px sans-serif`;
      ctx.textAlign = "center";
      ctx.fillStyle = "#fff";
      const metrics = ctx.measureText(text);
      ctx.fillText(text, Math.floor(x + width / 2), Math.floor(y + height / 2 + metrics.actualBoundingBoxAscent / 2));
    } else {
      ctx.beginPath();
      ctx.arc(size * 0.8, size * 0.8, size * 0.2, 0, 2 * Math.PI);
      ctx.fill();
    }
    return canvas.toDataURL("image/png");
  });

  $effect(() => {
    // Update both declarations so the ICO fallback cannot win over the live badge.
    const icons = document.querySelectorAll<HTMLLinkElement>('link[rel="icon"]');
    for (const icon of icons) {
      icon.href = href;
      icon.type = href.startsWith("data:image/png") ? "image/png" : "image/svg+xml";
      icon.removeAttribute("sizes");
      icon.title = authed ? label : "Monarch";
    }
  });
</script>
