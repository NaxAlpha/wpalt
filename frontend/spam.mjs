// Same-origin, bounded proof of work; no account or external reputation service.
export async function proof(resource, website = "") {
  const response = await fetch("/api/spam/challenge", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "same-origin",
    body: JSON.stringify({ resource }),
    cache: "no-store",
  });
  if (!response.ok)
    throw new Error("The local submission check is unavailable. Please retry.");
  const challenge = await response.json();
  if (
    !Number.isInteger(challenge.bits) ||
    challenge.bits < 0 ||
    challenge.bits > 16 ||
    typeof challenge.token !== "string" ||
    challenge.token.length > 256
  )
    throw new Error("Invalid local submission challenge.");
  const encoder = new TextEncoder();
  const deadline = performance.now() + 15000;
  for (let candidate = 0; candidate < 1048576; candidate++) {
    if (performance.now() > deadline)
      throw new Error("The submission check took too long. Please retry.");
    const solution = String(candidate);
    const hash = new Uint8Array(
      await crypto.subtle.digest(
        "SHA-256",
        encoder.encode(`${challenge.token}:${solution}`),
      ),
    );
    if (
      challenge.bits === 0 ||
      (hash[0] * 256 + hash[1]) >>> (16 - challenge.bits) === 0
    )
      return { token: challenge.token, solution, website };
    if (candidate % 256 === 0)
      await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error("Unable to finish the submission check. Please retry.");
}

for (const form of document.querySelectorAll("form[data-spam-resource]")) {
  form.addEventListener("submit", async (event) => {
    if (form.dataset.spamReady === "true") return;
    event.preventDefault();
    if (form.dataset.spamBusy === "true") return;
    form.dataset.spamBusy = "true";
    const button = form.querySelector(
      'button[type="submit"], button:not([type])',
    );
    const status = form.querySelector('[role="status"]');
    if (button) button.disabled = true;
    status.textContent = "Checking submission…";
    try {
      const answer = await proof(
        form.dataset.spamResource,
        form.elements.namedItem("website")?.value || "",
      );
      for (const name of ["token", "solution"]) {
        let input = form.elements.namedItem(name);
        if (!input) {
          input = document.createElement("input");
          input.type = "hidden";
          input.name = name;
          form.append(input);
        }
        input.value = answer[name];
      }
      form.dataset.spamReady = "true";
      if (button) button.disabled = false;
      form.requestSubmit();
    } catch (error) {
      status.textContent = error.message;
      if (button) button.disabled = false;
    } finally {
      delete form.dataset.spamBusy;
    }
  });
}
