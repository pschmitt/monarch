<script lang="ts">
  import { Compass } from "@lucide/svelte";
  import Empty from "../lib/components/Empty.svelte";
</script>

<div class="card">
  <Empty icon={Compass} title="Nothing here" body="This page doesn't exist — maybe the host was removed?">
    <a class="btn btn-primary" href="/">Back to overview</a>
  </Empty>
</div>
