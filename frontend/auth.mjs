// Browser WebAuthn transport only. Server owns challenges and verification.
const decode = (value) => {
  const text = atob(value.replaceAll("-", "+").replaceAll("_", "/"));
  return Uint8Array.from(text, (c) => c.charCodeAt(0));
};
const encode = (value) => {
  if (value === null) return null;
  return btoa(String.fromCharCode(...new Uint8Array(value)))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
};
async function request(path, value) {
  const response = await fetch(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(value),
  });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}
function options(challenge, register) {
  const publicKey = challenge.options.publicKey;
  publicKey.challenge = decode(publicKey.challenge);
  if (register) publicKey.user.id = decode(publicKey.user.id);
  for (const list of ["excludeCredentials", "allowCredentials"])
    for (const key of publicKey[list] || []) key.id = decode(key.id);
  return { publicKey };
}
function serialize(credential, register) {
  const response = {
    clientDataJSON: encode(credential.response.clientDataJSON),
  };
  if (register) {
    response.attestationObject = encode(credential.response.attestationObject);
    response.transports = credential.response.getTransports?.() || [];
  } else {
    response.authenticatorData = encode(credential.response.authenticatorData);
    response.signature = encode(credential.response.signature);
    response.userHandle = encode(credential.response.userHandle);
  }
  return {
    id: credential.id,
    rawId: encode(credential.rawId),
    type: credential.type,
    response,
    extensions: credential.getClientExtensionResults(),
  };
}
for (const form of document.querySelectorAll("[data-passkey]")) {
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const button = form.querySelector("button");
    const notice = form.querySelector("[role=status]");
    const register = form.dataset.passkey === "register";
    button.disabled = true;
    notice.textContent = "Waiting for your authenticator…";
    try {
      if (!window.PublicKeyCredential)
        throw new Error(
          "Passkeys require a supported browser and a secure origin.",
        );
      const data = Object.fromEntries(new FormData(form));
      const prefix = register ? "/account/passkeys" : "/passkeys/login";
      const challenge = await request(`${prefix}/start`, data);
      const credential = await navigator.credentials[
        register ? "create" : "get"
      ](options(challenge, register));
      if (!credential)
        throw new Error("Authenticator did not return a credential.");
      const outcome = await request(`${prefix}/finish`, {
        id: challenge.id,
        credential: serialize(credential, register),
        csrf: data.csrf || "",
      });
      location.assign(outcome.redirect);
    } catch (error) {
      notice.textContent =
        error.message ||
        "Passkey operation failed; retry with a new challenge.";
      button.disabled = false;
    }
  });
}
