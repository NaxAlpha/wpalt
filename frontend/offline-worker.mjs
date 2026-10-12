// Dedicated /offline/ scope; no authenticated navigation interception.
const VERSION = "wpalt-offline-v1";
const SHELL = `${VERSION}-${__WPALT_OFFLINE_BUILD__}-shell`,
  DATA = `${VERSION}-public`;
const EXPECTED = __WPALT_OFFLINE_ASSETS__;
const STATIC = [
  "/offline/",
  "/offline/app.js",
  "/offline/style.css",
  "/offline/forms.js",
];
const MAX = 512 * 1024,
  TOTAL = 2 * 1024 * 1024,
  COUNT = 30,
  TTL = 7 * 86400000;
const sourcePattern =
  /^(article|form)\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const endpoint = (source) =>
  `${self.location.origin}/offline/snapshots/${source}`;
let work = Promise.resolve();
let outstanding = 0;
async function bounded(response, limit) {
  if (!response.body) throw Error("The server returned no snapshot.");
  const reader = response.body.getReader();
  let size = 0,
    chunks = [];
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw Error("Offline storage budget exceeded.");
      chunks.push(value);
    }
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const c of chunks) {
    bytes.set(c, offset);
    offset += c.byteLength;
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}
async function fetchSnapshot(source) {
  const response = await fetch(endpoint(source), {
    credentials: "omit",
    redirect: "error",
    cache: "no-store",
    signal: AbortSignal.timeout(10000),
  });
  if (!response.ok) {
    await (await caches.open(DATA)).delete(endpoint(source));
    throw Error(
      "The live publication is unavailable or cannot be saved. Visit the live site.",
    );
  }
  if (
    response.type !== "basic" ||
    response.url !== endpoint(source) ||
    response.redirected ||
    response.headers.get("x-wpalt-offline") !== "public-snapshot-v1" ||
    !response.headers.get("content-type")?.startsWith("application/json")
  )
    throw Error("The server did not return an eligible public snapshot.");
  const value = JSON.parse(await bounded(response, MAX));
  if (
    value.format !== "wpalt-public-snapshot-v1" ||
    `${value.kind}/${value.id}` !== source ||
    !sourcePattern.test(source) ||
    typeof value.title !== "string" ||
    typeof value.text !== "string" ||
    !/^\/[a-zA-Z0-9/_-]+$/.test(value.source) ||
    !/^[0-9a-f]{64}$/.test(value.version)
  )
    throw Error("Invalid public snapshot.");
  return value;
}
async function records() {
  const cache = await caches.open(DATA),
    records = [];
  for (const key of await cache.keys()) {
    try {
      const r = JSON.parse(await bounded(await cache.match(key), MAX + 1024));
      if (
        !sourcePattern.test(r.source) ||
        key.url !== endpoint(r.source) ||
        !Number.isFinite(r.expires) ||
        r.expires <= Date.now() ||
        r.expires - r.saved !== TTL
      )
        throw Error();
      records.push(r);
    } catch {
      await cache.delete(key);
    }
  }
  return records;
}
self.addEventListener("install", (event) =>
  event.waitUntil(
    (async () => {
      const cache = await caches.open(SHELL);
      for (const route of STATIC) {
        const response = await fetch(route, {
          credentials: "omit",
          redirect: "error",
          cache: "no-store",
          signal: AbortSignal.timeout(10000),
        });
        if (!response.ok || response.redirected)
          throw Error("Offline shell unavailable");
        const body = await bounded(response, MAX);
        const hash = Array.from(
          new Uint8Array(
            await crypto.subtle.digest(
              "SHA-256",
              new TextEncoder().encode(body),
            ),
          ),
          (b) => b.toString(16).padStart(2, "0"),
        ).join("");
        if (hash !== EXPECTED[route])
          throw Error(
            "Offline shell changed during installation; reconnect and retry.",
          );
        await cache.put(
          route,
          new Response(body, {
            headers: { "content-type": response.headers.get("content-type") },
          }),
        );
      }
      await self.skipWaiting();
    })(),
  ),
);
self.addEventListener("activate", (event) =>
  event.waitUntil(
    (async () => {
      for (const name of await caches.keys())
        if (name.startsWith("wpalt-offline-") && ![SHELL, DATA].includes(name))
          await caches.delete(name);
      await records();
      await self.clients.claim();
    })(),
  ),
);
self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (
    event.request.method !== "GET" ||
    url.origin !== self.location.origin ||
    url.search ||
    !STATIC.includes(url.pathname)
  )
    return;
  event.respondWith(
    (async () => {
      // Immutable release shell is used together, rather than mixed-version network assets.
      const response = await (await caches.open(SHELL)).match(url.pathname);
      return response || fetch(event.request);
    })(),
  );
});
self.addEventListener("message", (event) => {
  const port = event.ports[0];
  if (!port || !event.source?.id) return;
  if (outstanding) {
    port.postMessage({
      error: "Local storage is busy. Try again after the current operation.",
    });
    return;
  }
  outstanding++;
  const operation = async () => {
    const client = await self.clients.get(event.source.id);
    if (!client) return;
    const url = new URL(client.url);
    if (url.origin !== self.location.origin || url.pathname !== "/offline/")
      throw Error("Use the offline library to manage storage.");
    const { action, source } = event.data || {};
    if (
      ["save", "get", "delete"].includes(action) &&
      !sourcePattern.test(source || "")
    )
      throw Error(
        "Use article/identifier or form/identifier from the live publication link.",
      );
    const cache = await caches.open(DATA),
      all = await records();
    if (action === "list")
      return all.map(({ snapshot, ...record }) => ({
        ...record,
        title: snapshot.title,
        kind: snapshot.kind,
      }));
    if (action === "clear") {
      await caches.delete(DATA);
      return true;
    }
    if (action === "delete") {
      await cache.delete(endpoint(source));
      return true;
    }
    if (action === "save") {
      const snapshot = await fetchSnapshot(source),
        saved = Date.now(),
        record = { source, saved, expires: saved + TTL, snapshot };
      const encoded = JSON.stringify(record),
        bytes = new TextEncoder().encode(encoded).length;
      const others = all.filter((r) => r.source !== source);
      const occupied = others.reduce(
        (n, r) => n + new TextEncoder().encode(JSON.stringify(r)).length,
        0,
      );
      if (others.length >= COUNT || bytes + occupied > TOTAL)
        throw Error(
          "Library full: delete a saved publication before saving this one.",
        );
      await cache.put(
        endpoint(source),
        new Response(encoded, {
          headers: { "content-type": "application/json" },
        }),
      );
      return record;
    }
    if (action === "get") {
      const record = all.find((r) => r.source === source);
      if (!record) throw Error("This publication is not saved or has expired.");
      try {
        const current = await fetchSnapshot(source);
        return {
          ...record,
          online: true,
          changed: current.version !== record.snapshot.version,
        };
      } catch (error) {
        if (error instanceof TypeError || error.name === "TimeoutError")
          return { ...record, online: false, changed: false };
        throw error;
      }
    }
    throw Error("Unknown library operation.");
  };
  const next = work.then(operation);
  work = next
    .catch(() => {})
    .finally(() => {
      outstanding--;
    });
  event.waitUntil(
    next
      .then((value) => port.postMessage({ value }))
      .catch((error) =>
        port.postMessage({
          error:
            error.message ||
            "Browser storage is unavailable. Your live site is still usable.",
        }),
      ),
  );
});
