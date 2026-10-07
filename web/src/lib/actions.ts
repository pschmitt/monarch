import { api } from "./api";
import { confirm, toast, toastError } from "./state.svelte";
import type { ServiceAction } from "./types";

export const ACTIONS: { id: ServiceAction; label: string; danger?: boolean }[] = [
  { id: "start", label: "Start" },
  { id: "stop", label: "Stop", danger: true },
  { id: "restart", label: "Restart" },
  { id: "monitor", label: "Monitor" },
  { id: "unmonitor", label: "Unmonitor", danger: true },
];

const verbs: Record<ServiceAction, string> = {
  start: "Start",
  stop: "Stop",
  restart: "Restart",
  monitor: "Enable monitoring for",
  unmonitor: "Disable monitoring for",
};

/** Ask for confirmation, run a monit action and report the outcome. Returns true on success. */
export async function runAction(hostId: number, hostLabel: string, services: string[], action: ServiceAction): Promise<boolean> {
  const what = services.length === 1 ? `“${services[0]}”` : `${services.length} services`;
  const ok = await confirm({
    title: `${verbs[action]} ${what}?`,
    body: `Monarch will ask Monit on ${hostLabel} to ${action} ${services.length === 1 ? "this service" : "these services"}. The new state shows up with the next report.`,
    confirm: ACTIONS.find((a) => a.id === action)!.label,
    danger: action === "stop" || action === "unmonitor",
  });
  if (!ok) return false;
  try {
    if (services.length === 1) await api.serviceAction(hostId, services[0], action);
    else await api.bulkAction(hostId, action, services);
    toast("ok", `${ACTIONS.find((a) => a.id === action)!.label} requested`, `${what} on ${hostLabel}`);
    return true;
  } catch (e) {
    toastError(e, `Could not ${action} ${what}`);
    return false;
  }
}
