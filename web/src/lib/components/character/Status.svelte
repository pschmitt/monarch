<script lang="ts">
  import { Check, X, Clock, TriangleAlert, Minus, ArrowRightLeft } from "@lucide/svelte";

  let { state, label }: { state: string; label?: string } = $props();
  const status = $derived.by(() => {
    switch (state) {
      case "ok": case "succeeded": return { tone: "ok", text: "Healthy", icon: Check };
      case "failed": return { tone: "bad", text: "Failed", icon: X };
      case "offline": return { tone: "bad", text: "Offline", icon: X };
      case "degraded": return { tone: "warn", text: "Needs attention", icon: TriangleAlert };
      case "pending": case "init": return { tone: "warn", text: state === "init" ? "Starting" : "Pending", icon: Clock };
      case "changed": return { tone: "info", text: "Changed", icon: ArrowRightLeft };
      case "unmonitored": return { tone: "muted", text: "Unmonitored", icon: Minus };
      case "unchanged": case "changed_not": return { tone: "muted", text: "Unchanged", icon: Minus };
      default: return { tone: "muted", text: "Stale", icon: Minus };
    }
  });
</script>

<span class="c-status c-status-{status.tone}">
  <status.icon size={16} strokeWidth={1.8} aria-hidden="true" />
  {label ?? status.text}
</span>
