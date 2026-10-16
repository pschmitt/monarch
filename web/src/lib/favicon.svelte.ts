export type FaviconMode = "count" | "binary";

function initialMode(): FaviconMode {
  try {
    return localStorage.getItem("monarch.favicon") === "binary" ? "binary" : "count";
  } catch {
    return "count";
  }
}

export const faviconPreference = $state<{ mode: FaviconMode }>({ mode: initialMode() });

export function setFaviconMode(mode: FaviconMode) {
  faviconPreference.mode = mode;
  try {
    localStorage.setItem("monarch.favicon", mode);
  } catch {
    /* Keep the preference for this session when storage is unavailable. */
  }
}
