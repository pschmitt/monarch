# Fleet status favicon

The browser tab overlays the Monarch crown with a green dot when the fleet is
healthy, or a red issue count (capped at `99+`). Each offline host counts as one
issue; online hosts contribute their failed services, with at least one issue
for a degraded host. An amber `!` means status is unavailable, including initial
loading, connection loss, or a failed fleet refresh. Signing out restores the
plain crown.

Use **User menu → Favicon** to switch between the issue count and binary
red/green dots. This preference is saved in the current browser and is available
to every dashboard user.
