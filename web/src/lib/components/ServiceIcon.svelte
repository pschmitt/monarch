<script lang="ts">
  import { ArrowRightLeft, Cpu, File, Folder, Globe, HardDrive, Network, Server, SquareTerminal } from "@lucide/svelte";
  import type { ServiceType } from "../types";
  let { type, size = 16 }: { type: ServiceType | null; size?: number } = $props();
  const icons = {
    system: Server,
    process: Cpu,
    filesystem: HardDrive,
    directory: Folder,
    file: File,
    fifo: ArrowRightLeft,
    host: Globe,
    program: SquareTerminal,
    net: Network,
  } as const;
  const Icon = $derived(type ? icons[type] : Server);
</script>

<Icon {size} strokeWidth={1.75} />
