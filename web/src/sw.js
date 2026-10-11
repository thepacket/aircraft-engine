// Service worker: makes the simulator usable offline after the first visit.
// The build injects the fingerprinted application files (JS, CSS, worker,
// wasm, scenarios) into PRECACHE and a cache name derived from them, so the
// whole app is cached at install time, before the page's own requests, rather
// than relying on each file having been fetched once while online.
// Same-origin GET requests are served network-first with the cache as the
// fallback; new deployments change the cache name and old entries are dropped.
const PRECACHE = self.__PRECACHE__ || ["./", "./manifest.webmanifest"];
const CACHE = "aircraft-engine-" + (self.__BUILD__ || "dev");

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(PRECACHE)));
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
