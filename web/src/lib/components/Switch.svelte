<script lang="ts">
  let {
    checked = $bindable(false),
    label = "",
    disabled = false,
    onchange,
  }: { checked?: boolean; label?: string; disabled?: boolean; onchange?: (v: boolean) => void } = $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors disabled:opacity-40 {checked
    ? 'border-transparent bg-accent-gradient'
    : 'border-line-strong bg-surface-2'}"
  onclick={() => {
    checked = !checked;
    onchange?.(checked);
  }}
>
  <span class="inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform {checked ? 'translate-x-[18px]' : 'translate-x-[2px]'}"></span>
</button>
