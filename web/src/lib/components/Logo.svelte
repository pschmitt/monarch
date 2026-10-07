<script lang="ts">
  let { size = 28, wordmark = false }: { size?: number; wordmark?: boolean } = $props();
  const id = `lg-${Math.random().toString(36).slice(2, 8)}`;
</script>

<span class="inline-flex items-center gap-2.5">
  <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true" class="shrink-0">
    <defs>
      <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#b4a5ff" />
        <stop offset="0.55" stop-color="#818cf8" />
        <stop offset="1" stop-color="#2dd4f0" />
      </linearGradient>
    </defs>
    <rect width="64" height="64" rx="16" fill="var(--surface-solid)" stroke="var(--line-strong)" />
    <path d="M12 42 L15.5 21 L24.5 30 L32 15 L39.5 30 L48.5 21 L52 42 Z" fill="url(#{id})" />
    <rect x="12" y="45.5" width="40" height="5" rx="2.5" fill="url(#{id})" opacity="0.85" />
    <circle cx="32" cy="15" r="3.2" fill="#e0f2fe" />
  </svg>
  {#if wordmark}
    <span class="text-[15px] font-semibold tracking-tight text-fg">Monarch</span>
  {/if}
</span>
