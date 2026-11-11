export type FaviconMode = "count" | "binary";
export type FaviconCountSource = "issues" | "hosts" | "services";

function initialMode(): FaviconMode {
  try {
    return localStorage.getItem("monarch.favicon") === "binary" ? "binary" : "count";
  } catch {
    return "count";
  }
}

function initialCountSource(): FaviconCountSource {
  try {
    const source = localStorage.getItem("monarch.favicon-count-source");
    return source === "hosts" || source === "services" || source === "issues" ? source : "issues";
  } catch {
    return "issues";
  }
}

export const faviconPreference = $state<{ mode: FaviconMode; countSource: FaviconCountSource }>({
  mode: initialMode(),
  countSource: initialCountSource(),
});

export function setFaviconMode(mode: FaviconMode) {
  faviconPreference.mode = mode;
  try {
    localStorage.setItem("monarch.favicon", mode);
  } catch {
    /* Keep the preference for this session when storage is unavailable. */
  }
}

export function setFaviconCountSource(source: FaviconCountSource) {
  if (source !== "issues" && source !== "hosts" && source !== "services") return;
  faviconPreference.countSource = source;
  try {
    localStorage.setItem("monarch.favicon-count-source", source);
  } catch {
    /* Keep the preference for this session when storage is unavailable. */
  }
}
