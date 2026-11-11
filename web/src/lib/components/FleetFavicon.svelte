<script lang="ts">
  import { onMount } from "svelte";
  import crown from "../../../public/favicon.svg?raw";
  import { faviconPreference } from "../favicon.svelte";
  import { activeFailureCount, activeFailingHostCount, activeFailingServiceCount } from "../silence";
  import { clock, fleet } from "../state.svelte";

  let { authed }: { authed: boolean } = $props();

  const hosts = $derived(Object.values(fleet.hosts));
  const issues = $derived(hosts.reduce((total, host) => total + activeFailureCount(host, clock.now), 0));
  const unhealthyHosts = $derived(activeFailingHostCount(hosts, clock.now));
  const failingServices = $derived(activeFailingServiceCount(hosts, clock.now));
  const offlineHosts = $derived(hosts.filter((host) => (!host.online || host.state === "offline") && activeFailureCount(host, clock.now) > 0).length);
  const count = $derived(faviconPreference.countSource === "hosts" ? unhealthyHosts : faviconPreference.countSource === "services" ? failingServices : issues);
  const unknown = $derived(!fleet.loaded || fleet.loadError || fleet.status !== "live");
  const label = $derived.by(() => {
    if (unknown) return "Fleet status unavailable";
    if (!issues) return "Fleet healthy";
    if (faviconPreference.countSource === "hosts") {
      return `${count} unhealthy host${count === 1 ? "" : "s"}${offlineHosts ? `, including ${offlineHosts} offline host${offlineHosts === 1 ? "" : "s"}` : ""}`;
    }
    if (faviconPreference.countSource === "services") {
      return `${count} active failing service${count === 1 ? "" : "s"}${offlineHosts ? `; ${offlineHosts} offline host issue${offlineHosts === 1 ? "" : "s"} not included in this count` : ""}`;
    }
    return `${count} fleet issue${count === 1 ? "" : "s"}; offline hosts count as one issue each`;
  });
  let baseImage = $state<HTMLImageElement | null>(null);
  onMount(() => {
    const image = new Image();
    image.onload = () => (baseImage = image);
    image.src = `data:image/svg+xml,${encodeURIComponent(crown)}`;
    return () => { image.onload = null; };
  });

  const href = $derived.by(() => {
    if (!authed) return "/favicon.svg";
    const text = unknown ? "!" : issues && faviconPreference.mode === "count" ? (count > 99 ? "99+" : String(count)) : "";
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
