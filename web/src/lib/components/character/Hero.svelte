<script lang="ts">
  import type { Snippet } from "svelte";
  import { Crown, type Server } from "@lucide/svelte";

  let { title, description, icon: Icon, children }: {
    title: string;
    description?: string;
    icon?: typeof Server;
    children?: Snippet;
  } = $props();
</script>

<header class="c-hero">
  <div class="c-hero-copy">
    <h1 class="c-title c-hero-title">
      {#if Icon}<Icon size={30} strokeWidth={1.75} />{/if}
      {title}
    </h1>
    {#if description}<p>{description}</p>{/if}
    {#if children}{@render children()}{/if}
  </div>
  <Crown class="c-hero-crown" size={150} strokeWidth={1.5} aria-hidden="true" />
</header>
