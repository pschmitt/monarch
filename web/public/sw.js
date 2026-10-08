// Service worker for browser notifications (Web Push).
self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data ? event.data.json() : {};
  } catch {
    data = { title: "Monarch", text: event.data ? event.data.text() : "" };
  }
  const title = data.title || "Monarch";
  event.waitUntil(
    self.registration.showNotification(title, {
      body: data.text || "",
      icon: "/icon-192.png",
      badge: "/icon-192.png",
      tag: data.url || title,
      data: { url: data.url || "/" },
      requireInteraction: !!data.failed,
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = (event.notification.data && event.notification.data.url) || "/";
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((list) => {
      for (const c of list) {
        if (c.url === url && "focus" in c) return c.focus();
      }
      return self.clients.openWindow(url);
    }),
  );
});
