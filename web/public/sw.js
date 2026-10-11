// Service worker: makes the simulator usable offline after the first visit.
// Same-origin GET requests are served network-first and the response is kept
// in the cache; when the network fails (offline), the cached copy is used.
// The built app is fingerprinted by Vite, so a new deployment simply adds new
// files; old entries are dropped when the cache name changes.
const CACHE = "aircraft-engine-v1";

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(["./", "./manifest.webmanifest"]).catch(() => undefined)));
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))));
  self.clients.claim();
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET" || new URL(req.url).origin !== self.location.origin) return;
  event.respondWith(
    fetch(req)
      .then((res) => {
        if (res.ok) { const copy = res.clone(); caches.open(CACHE).then((c) => c.put(req, copy)); }
        return res;
      })
      .catch(() => caches.match(req, { ignoreSearch: req.mode === "navigate" }).then((hit) => hit ?? (req.mode === "navigate" ? caches.match("./") : undefined)))
  );
});
