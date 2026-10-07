<script lang="ts">
  import { Check, Copy } from "@lucide/svelte";
  let { code, label = "" }: { code: string; label?: string } = $props();
  let copied = $state(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = code;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }
</script>

<div class="overflow-hidden rounded-xl border border-line-strong bg-[color-mix(in_oklab,var(--bg)_80%,transparent)]">
  <div class="flex items-center justify-between border-b border-line px-3.5 py-2">
    <span class="num text-[11px] text-fg-3">{label}</span>
    <button class="btn btn-ghost btn-sm" onclick={copy} aria-label="Copy to clipboard">
      {#if copied}<Check size={13} class="text-ok" /> Copied{:else}<Copy size={13} /> Copy{/if}
    </button>
  </div>
  <pre class="num overflow-x-auto p-4 text-[12.5px] leading-relaxed text-fg">{code}</pre>
</div>
