<script lang="ts">
  import { History } from "@lucide/svelte";
  import EventsList from "../lib/components/EventsList.svelte";
</script>

<div class="space-y-5">
  <div>
    <h1 class="flex items-center gap-2.5 text-2xl font-semibold tracking-tight text-fg"><History size={23} class="text-accent-2" /> Events</h1>
    <p class="mt-1 text-[13px] text-fg-3">Host and service changes, newest first.</p>
  </div>
  <EventsList />
</div>
