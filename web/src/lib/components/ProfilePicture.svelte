<script lang="ts">
  import { ImageUp, LoaderCircle, Trash } from "@lucide/svelte";
  import { api } from "../api";
  import { refreshSession, session, toast, toastError } from "../state.svelte";
  import type { User } from "../types";
  import Avatar from "./Avatar.svelte";
  import Modal from "./Modal.svelte";

  // Without `target` this edits the signed-in user's own picture; admins pass
  // another account (e.g. a collector that never signs in).
  let { open = $bindable(false), target = null, onsaved }: { open?: boolean; target?: User | null; onsaved?: (u: User) => void } = $props();

  const SIZE = 256;
  let input = $state<HTMLInputElement | null>(null);
  let picked = $state<Blob | null>(null);
  let preview = $state<string | null>(null);
  let busy = $state(false);
  const user = $derived(target ?? session.me?.user ?? null);
  const own = $derived(!target || target.id === session.me?.user?.id);

  async function done(u: User, message: string) {
    if (own) await refreshSession();
    onsaved?.(u);
    toast("ok", message);
    open = false;
  }

  $effect(() => {
    if (!open) reset();
  });

  function reset() {
    if (preview) URL.revokeObjectURL(preview);
    picked = null;
    preview = null;
    if (input) input.value = "";
  }

  // Center-crop to a square and scale down, so uploads stay small and the
  // server never has to decode images.
  async function crop(file: File): Promise<Blob> {
    const img = await createImageBitmap(file);
    const side = Math.min(img.width, img.height);
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = Math.min(SIZE, side);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas unavailable");
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, (img.width - side) / 2, (img.height - side) / 2, side, side, 0, 0, canvas.width, canvas.height);
    img.close();
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/webp", 0.9));
    // Browsers without WebP encoding fall back to PNG.
    return blob ?? (await new Promise<Blob>((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("could not encode image"))), "image/png")));
  }

  async function onpick(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    try {
      const blob = await crop(file);
      if (preview) URL.revokeObjectURL(preview);
      picked = blob;
      preview = URL.createObjectURL(blob);
    } catch (err) {
      toastError(err, "Could not read that image");
    }
  }

  async function save() {
    if (!picked) return;
    busy = true;
    try {
      await done(await api.uploadAvatar(picked, own ? "me" : target!.id), "Profile picture updated");
    } catch (err) {
      toastError(err, "Could not upload profile picture");
    } finally {
      busy = false;
    }
  }

  async function remove() {
    busy = true;
    try {
      await done(await api.deleteAvatar(own ? "me" : target!.id), "Profile picture removed");
    } catch (err) {
      toastError(err, "Could not remove profile picture");
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title={own ? "Profile picture" : `Profile picture of ${target?.username}`} width="max-w-sm">
  <div class="flex flex-col items-center gap-4">
    {#if preview}
      <img src={preview} alt="Preview" class="h-28 w-28 rounded-full object-cover ring-1 ring-[var(--mn-line)]" />
    {:else}
      <Avatar {user} size={112} />
    {/if}
    <input bind:this={input} id="pp-file" class="sr-only" type="file" accept="image/png,image/jpeg,image/webp,image/gif" onchange={onpick} />
    <label for="pp-file" class="btn btn-sm cursor-pointer"><ImageUp size={14} /> Choose image…</label>
    <p class="hint text-center">Cropped to a square and scaled to {SIZE}×{SIZE}. Visible to everyone signed in to Monarch.{#if user?.avatar === 0} Currently shown: Gravatar.{/if}</p>
  </div>
  {#snippet footer()}
    {#if user?.avatar && !picked}
      <button class="btn btn-ghost mr-auto text-bad" onclick={remove} disabled={busy}><Trash size={14} /> Remove</button>
    {/if}
    <button class="btn btn-ghost" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" onclick={save} disabled={busy || !picked}>{#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Save</button>
  {/snippet}
</Modal>
