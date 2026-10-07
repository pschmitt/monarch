<script lang="ts">
  import EventsList from "../lib/components/EventsList.svelte";
</script>

<div class="space-y-5">
  <div>
    <h1 class="text-2xl font-semibold tracking-tight text-fg">Events</h1>
    <p class="mt-1 text-[13px] text-fg-3">State changes reported by Monit and heartbeat alerts raised by Monarch.</p>
  </div>
  <EventsList />
</div>
