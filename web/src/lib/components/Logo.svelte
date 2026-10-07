<script lang="ts">
  let { size = 28, wordmark = false }: { size?: number; wordmark?: boolean } = $props();
  const id = `lg-${Math.random().toString(36).slice(2, 8)}`;
</script>

<span class="inline-flex items-center gap-2.5">
  <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true" class="shrink-0 drop-shadow-[0_4px_12px_rgb(111_124_244/0.35)]">
    <defs>
      <linearGradient id={id} x1="6" y1="2" x2="58" y2="62" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#9a86ff" />
        <stop offset="0.5" stop-color="#6f7cf4" />
        <stop offset="1" stop-color="#1fc8e6" />
      </linearGradient>
      <linearGradient id="{id}-s" x1="0" y1="0" x2="0" y2="64" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#fff" stop-opacity=".22" />
        <stop offset=".5" stop-color="#fff" stop-opacity="0" />
      </linearGradient>
    </defs>
    <rect width="64" height="64" rx="15" fill="url(#{id})" />
    <rect width="64" height="64" rx="15" fill="url(#{id}-s)" />
    <g transform="translate(0 -1.5)">
      <path d="M13.5 43.5 L16.5 21 L25.5 30.5 L32 15.5 L38.5 30.5 L47.5 21 L50.5 43.5 Z" fill="#fff" stroke="#fff" stroke-width="2.5" stroke-linejoin="round" />
      <rect x="13" y="47" width="38" height="5.5" rx="2.75" fill="#fff" />
    </g>
  </svg>
  {#if wordmark}
    <span class="text-[15px] font-semibold tracking-tight text-fg">Monarch</span>
  {/if}
</span>
