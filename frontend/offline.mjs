const status = document.querySelector("#status"),
  library = document.querySelector("#library"),
  reading = document.querySelector("#reading");
let worker;
function notice(text, error = false) {
  status.textContent = text;
  status.setAttribute("role", error ? "alert" : "status");
}
function clearReading() {
  const host = reading.querySelector("#public-form");
  if (host)
    document.dispatchEvent(
      new CustomEvent("wpalt:offline-form-remove", { detail: host }),
    );
  reading.replaceChildren();
  reading.hidden = true;
}
function element(tag, text) {
  const n = document.createElement(tag);
  if (text !== undefined) n.textContent = text;
  return n;
}
function command(action, source) {
  return new Promise((resolve, reject) => {
    if (!worker)
      return reject(
        Error("Offline storage is unavailable. Visit the live site."),
      );
    const channel = new MessageChannel(),
      timer = setTimeout(() => {
        channel.port1.close();
        reject(Error("Local storage did not respond. Try again."));
      }, 15000);
    channel.port1.onmessage = ({ data }) => {
      clearTimeout(timer);
      channel.port1.close();
      data.error ? reject(Error(data.error)) : resolve(data.value);
    };
    worker.postMessage({ action, source }, [channel.port2]);
  });
}
async function list() {
  const all = await command("list");
  library.replaceChildren();
  document.querySelector("#usage").textContent =
    `${all.length} of 30 saved publications · 2 MiB total snapshot budget`;
  for (const r of all) {
    const li = element("li"),
      a = element("a", r.title);
    a.href = `#open/${r.source}`;
    li.append(
      a,
      element(
        "p",
        `Saved ${new Date(r.saved).toLocaleString()} · expires ${new Date(r.expires).toLocaleString()}`,
      ),
    );
    const remove = element("button", "Delete");
    remove.type = "button";
    remove.className = "secondary";
    remove.setAttribute("aria-label", `Delete ${r.title}`);
    remove.onclick = async () => {
      try {
        await command("delete", r.source);
        if (r.kind === "form")
          localStorage.removeItem(`wpalt:form:${r.source.split("/")[1]}`);
        clearReading();
        await list();
        notice("Saved publication deleted.");
      } catch (e) {
        notice(e.message, true);
      }
    };
    li.append(remove);
    library.append(li);
  }
}
async function open(source) {
  const record = await command("get", source),
    s = record.snapshot;
  clearReading();
  reading.hidden = false;
  reading.append(
    element("h2", s.title),
    element(
      "p",
      `${record.online ? "Online check completed" : "Offline copy; current publication cannot be checked"} · saved ${new Date(record.saved).toLocaleString()}`,
    ),
  );
  if (record.changed)
    reading.append(
      element(
        "p",
        "The live publication changed. Your saved source and original draft remain unchanged. Review online or deliberately save a new copy.",
      ),
    );
  const link = element(
    "a",
    s.kind === "form"
      ? "Reconnect and review on the live form"
      : "Open original source online",
  );
  link.href = s.source;
  reading.append(link);
  const prose = element("p", s.text);
  prose.className = "prose";
  prose.lang = s.locale;
  reading.append(prose);
  if (s.kind === "form") {
    const host = element("div");
    host.id = "public-form";
    Object.assign(host.dataset, {
      id: s.id,
      version: String(s.form_version),
      definition: JSON.stringify(s.definition),
      offline: "true",
      spam: "false",
      scanning: "false",
    });
    reading.append(host);
    document.dispatchEvent(
      new CustomEvent("wpalt:offline-form", { detail: host }),
    );
  }
  reading.scrollIntoView({
    behavior: matchMedia("(prefers-reduced-motion: reduce)").matches
      ? "auto"
      : "smooth",
    block: "start",
  });
}
async function hash() {
  const match = location.hash.match(
    /^#(open\/)?((article|form)\/[0-9a-f-]{36})$/,
  );
  if (!match) return;
  if (match[1]) await open(match[2]);
  else {
    document.querySelector("#source").value = match[2];
    notice(
      "Choose Save on this device to keep this publication. Nothing has been saved yet.",
    );
  }
}
document.querySelector("#save").onsubmit = async (e) => {
  e.preventDefault();
  const button = e.currentTarget.querySelector("button");
  button.disabled = true;
  try {
    const source = document.querySelector("#source").value.trim();
    await command("save", source);
    await list();
    notice("Saved privately on this device for seven days.");
    location.hash = `open/${source}`;
  } catch (e) {
    notice(e.message, true);
  } finally {
    button.disabled = false;
  }
};
document.querySelector("#clear").onclick = async () => {
  try {
    await command("clear");
    for (const key of Object.keys(localStorage))
      if (key.startsWith("wpalt:form:")) localStorage.removeItem(key);
    clearReading();
    history.replaceState(null, "", "/offline/");
    await list();
    notice("Library and device form drafts cleared.");
  } catch (e) {
    notice(e.message, true);
  }
};
window.addEventListener("hashchange", () =>
  hash().catch((e) => {
    clearReading();
    notice(e.message, true);
  }),
);
(async () => {
  try {
    if (!isSecureContext || !("serviceWorker" in navigator))
      throw Error(
        "Offline storage requires HTTPS or localhost and a supporting browser. Visit the live site.",
      );
    const registration = await navigator.serviceWorker.register(
      "/offline/worker.js",
      { scope: "/offline/", updateViaCache: "none" },
    );
    let readyTimer;
    try {
      await Promise.race([
        navigator.serviceWorker.ready,
        new Promise((_, reject) => {
          readyTimer = setTimeout(
            () =>
              reject(
                Error(
                  "Offline storage could not be prepared. Reconnect and retry, or visit the live site.",
                ),
              ),
            20000,
          );
        }),
      ]);
    } finally {
      clearTimeout(readyTimer);
    }
    worker = registration.active;
    await list();
    notice("Ready. Only publications you deliberately save are retained.");
    await hash();
  } catch (e) {
    notice(e.message, true);
  }
})();
