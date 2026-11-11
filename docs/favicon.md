# Fleet status favicon

The browser tab overlays the Monarch crown with a green dot when the fleet is
healthy, or a red count (capped at `99+`) when it has active issues. The default
**Fleet issues** count preserves the original behavior: each active failed
service on an online host counts once, and each offline host counts once.
Silenced hosts and services do not contribute to the count. An amber `!` means
status is unavailable, including initial loading, connection loss, or a failed
fleet refresh. Signing out restores the plain crown.

Use **Settings → Appearance → Favicon** to switch between the issue count and
binary red/green dots. In count mode, choose what the number represents:

- **Fleet issues** counts active failed services and offline hosts.
- **Hosts** counts distinct unhealthy hosts, including offline hosts.
- **Services** counts active failed services on online hosts. Offline hosts and
  their stale service states are excluded from this number, but they still keep
  the icon red; the icon title explains how many offline host issues are not
  included.

The selected count source is saved in the current browser and is available to
every dashboard user. Binary mode keeps the same overall fleet-health status
regardless of the selected count source.
