<script lang="ts">
  import { avatarUrl } from "../api";
  import type { User } from "../types";

  let { user, size = 28 }: { user: Pick<User, "id" | "username" | "avatar"> | null | undefined; size?: number } = $props();

  const src = $derived(user ? avatarUrl(user) : null);
  let failed = $state<string | null>(null);
</script>

{#if src && failed !== src}
  <img
    {src}
    alt=""
    width={size}
    height={size}
    class="shrink-0 rounded-full object-cover ring-1 ring-[var(--line)]"
    style="width:{size}px;height:{size}px"
    onerror={() => (failed = src)}
  />
{:else}
  <span
    class="bg-accent-gradient flex shrink-0 items-center justify-center rounded-full font-bold text-white uppercase"
    style="width:{size}px;height:{size}px;font-size:{Math.max(9, Math.round(size * 0.38))}px"
    aria-hidden="true"
  >
    {user?.username?.slice(0, 2) ?? "?"}
  </span>
{/if}
