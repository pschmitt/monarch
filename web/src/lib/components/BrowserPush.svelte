<script lang="ts">
  import { Bell, BellOff, LoaderCircle, Send } from "@lucide/svelte";
  import { api } from "../api";
  import { toast, toastError } from "../state.svelte";

  const supported = typeof navigator !== "undefined" && "serviceWorker" in navigator && "PushManager" in window && "Notification" in window;

  let busy = $state(false);
  let permission = $state<NotificationPermission>(supported ? Notification.permission : "denied");
  let subscribed = $state(false);
  let subId = $state<number | null>(null);

  function b64(buf: ArrayBuffer | null): string {
    if (!buf) return "";
    return btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }
  function b64ToBytes(s: string): Uint8Array<ArrayBuffer> {
    const pad = "=".repeat((4 - (s.length % 4)) % 4);
    const raw = atob((s + pad).replace(/-/g, "+").replace(/_/g, "/"));
    return Uint8Array.from(raw, (c) => c.charCodeAt(0));
  }

  async function registration() {
    return (await navigator.serviceWorker.getRegistration()) ?? (await navigator.serviceWorker.register("/sw.js"));
  }

  async function refresh() {
    if (!supported) return;
    try {
      const reg = await registration();
      const sub = await reg.pushManager.getSubscription();
      subscribed = !!sub;
      subId = null;
      if (sub) {
        const mine = await api.pushSubscriptions();
        subId = mine.find((m) => m.endpoint === sub.endpoint)?.id ?? null;
        // Re-register with the server if it forgot this browser (e.g. after a user switch).
        if (subId === null) await enable(true);
      }
    } catch {
      /* not available (private window, http, ...) */
    }
  }

  $effect(() => {
    refresh();
  });

  async function enable(silent = false) {
    busy = true;
    try {
      permission = await Notification.requestPermission();
      if (permission !== "granted") {
        if (!silent) toast("warn", "Notifications are blocked", "Allow them for this site in the browser settings.");
        return;
      }
      const reg = await registration();
      const { public_key } = await api.pushKey();
      const sub =
        (await reg.pushManager.getSubscription()) ??
        (await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: b64ToBytes(public_key) }));
      const created = await api.pushSubscribe({
        endpoint: sub.endpoint,
        keys: { p256dh: b64(sub.getKey("p256dh")), auth: b64(sub.getKey("auth")) },
        user_agent: navigator.userAgent,
      });
      subscribed = true;
      subId = created.id;
      if (!silent) toast("ok", "Browser notifications enabled");
    } catch (e) {
      toastError(e, "Could not enable browser notifications");
    } finally {
      busy = false;
    }
  }

  async function disable() {
    busy = true;
    try {
      const reg = await registration();
      const sub = await reg.pushManager.getSubscription();
      if (subId !== null) await api.pushUnsubscribe(subId);
      await sub?.unsubscribe();
      subscribed = false;
      subId = null;
      toast("ok", "Browser notifications disabled");
    } catch (e) {
      toastError(e, "Could not disable browser notifications");
    } finally {
      busy = false;
    }
  }

  async function test() {
    busy = true;
    try {
      const r = await api.pushTest();
      toast(r.ok ? "ok" : "bad", r.ok ? "Test notification sent" : "Test failed", r.errors.join("; "));
    } catch (e) {
      toastError(e, "Test failed");
    } finally {
      busy = false;
    }
  }
</script>

<div class="card flex flex-wrap items-center gap-3 p-4">
  <span class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-line-strong bg-surface-2 text-accent">
    {#if subscribed}<Bell size={18} />{:else}<BellOff size={18} />{/if}
  </span>
  <div class="min-w-0 flex-1">
    <div class="text-[14px] font-semibold text-fg">Notifications on this device</div>
    <p class="text-xs text-fg-3">
      {#if !supported}
        This browser does not support push notifications (they need HTTPS and a service worker).
      {:else if permission === "denied"}
        Blocked in the browser settings for this site.
      {:else if subscribed}
        Enabled. To get them for alerts, add a “Browser push” channel below.
      {:else}
        Get alerts as system notifications, even when Monarch is closed.
      {/if}
    </p>
  </div>
  {#if supported && permission !== "denied"}
    {#if subscribed}
      <button class="btn btn-sm" onclick={test} disabled={busy}>{#if busy}<LoaderCircle size={13} class="animate-spin" />{:else}<Send size={13} />{/if} Test</button>
      <button class="btn btn-ghost btn-sm" onclick={disable} disabled={busy}>Disable</button>
    {:else}
      <button class="btn btn-primary btn-sm" onclick={() => enable()} disabled={busy}>{#if busy}<LoaderCircle size={13} class="animate-spin" />{/if} Enable</button>
    {/if}
  {/if}
</div>
