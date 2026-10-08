// Installable web app: register the service worker at startup and keep the browser's
// install prompt around so Settings > About can offer an "Install app" button.

interface InstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

let deferred: InstallPromptEvent | null = null;

export const pwa = $state({
  /** The browser offers to install right now. */
  canInstall: false,
  /** Already running as an installed app. */
  installed:
    typeof matchMedia === "function" &&
    (matchMedia("(display-mode: standalone)").matches || (navigator as { standalone?: boolean }).standalone === true),
});

export function initPwa() {
  if ("serviceWorker" in navigator) {
    window.addEventListener("load", () => {
      navigator.serviceWorker.register("/sw.js").catch(() => {
        /* http, private window, ... */
      });
    });
  }
  window.addEventListener("beforeinstallprompt", (e) => {
    e.preventDefault();
    deferred = e as InstallPromptEvent;
    pwa.canInstall = true;
  });
  window.addEventListener("appinstalled", () => {
    deferred = null;
    pwa.canInstall = false;
    pwa.installed = true;
  });
}

export async function installApp() {
  if (!deferred) return;
  const ev = deferred;
  deferred = null;
  pwa.canInstall = false;
  await ev.prompt();
  await ev.userChoice;
}
