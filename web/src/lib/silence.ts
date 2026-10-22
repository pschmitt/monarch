import type { HostSummary } from "./types";

type SilencedHost = Pick<HostSummary, "online" | "state" | "failing" | "muted_services" | "muted_until">;

export function isHostSilenced(host: SilencedHost, now: number): boolean {
  return host.muted_until === "forever" || (typeof host.muted_until === "number" && host.muted_until > now);
}

export function activeFailureCount(host: SilencedHost, now: number): number {
  if (isHostSilenced(host, now)) return 0;
  if (!host.online || host.state === "offline") return 1;
  const muted = new Set(host.muted_services);
  return host.failing.filter((service) => !muted.has(service)).length;
}

export function activeFailingHostCount(hosts: HostSummary[], now: number): number {
  return hosts.filter((host) => activeFailureCount(host, now) > 0).length;
}

export function activeFailingServiceCount(hosts: HostSummary[], now: number): number {
  return hosts.reduce((count, host) => {
    if (isHostSilenced(host, now) || !host.online || host.state === "offline") return count;
    const muted = new Set(host.muted_services);
    return count + host.failing.filter((service) => !muted.has(service)).length;
  }, 0);
}
