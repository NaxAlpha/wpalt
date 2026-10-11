// Shared working-copy controls. No outside preview service or remote asset URL.
const form = document.querySelector("[data-editor]");
if (form?.querySelector("[data-discovery-preview]")) {
  const read = (name) => form.elements.namedItem(name)?.value || "";
  const preview = form.querySelector("[data-discovery-preview]");
  function update() {
    const video = {
      media: read("seo_video_media"),
      poster: read("seo_video_poster"),
      title: read("seo_video_title"),
      description: read("seo_video_description"),
      sitemap: form.elements.namedItem("seo_video_sitemap").checked,
    };
    const news = {
      publication: read("seo_news_publication"),
      language: read("seo_news_language"),
    };
    const seo = {
      title: read("seo_title"),
      description: read("seo_description"),
      noindex: form.elements.namedItem("seo_noindex").checked,
      schema_type: read("seo_type"),
      social_image: read("seo_social_image"),
      social_alt: read("seo_social_alt"),
      video:
        [video.media, video.poster, video.title, video.description].some(
          Boolean,
        ) || video.sitemap
          ? video
          : null,
      news: Object.values(news).some(Boolean) ? news : null,
    };
    const incomplete = Boolean(
      (seo.video &&
        ![
          video.media,
          video.poster,
          video.title.trim(),
          video.description.trim(),
        ].every(Boolean)) ||
      (seo.news &&
        (!news.publication.trim() ||
          !news.language ||
          seo.schema_type !== "Article")),
    );
    form.dataset.discoveryIncomplete = String(incomplete);
    form.querySelector("[data-seo-json]").value = JSON.stringify(seo);
    preview.querySelector("[data-preview-title]").textContent =
      seo.title || read("title") || "Untitled working copy";
    preview.querySelector("[data-preview-description]").textContent =
      seo.description || "The platform may choose a snippet from your content.";
    const language = read("locale");
    const prefix =
      language === preview.dataset.defaultLanguage ? "" : "/" + language;
    preview.querySelector("[data-preview-url]").textContent =
      form.dataset.origin + prefix + "/" + read("slug");
    preview.querySelector("[data-preview-indexing]").textContent = seo.noindex
      ? "Excluded from search indexing when published"
      : "Available for search indexing when published";
    const image = preview.querySelector("[data-preview-image]");
    image.alt = seo.social_alt;
    if (
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
        seo.social_image,
      )
    ) {
      const path = "/media/" + seo.social_image;
      if (image.getAttribute("src") !== path) image.src = path;
      image.hidden = false;
    } else {
      image.removeAttribute("src");
      image.hidden = true;
    }
  }
  form.addEventListener("input", update);
  form.addEventListener("change", update);
  update();
  const removeVideo = form.querySelector("[data-discovery-remove-video]");
  removeVideo.hidden = false;
  removeVideo.addEventListener("click", () => {
    for (const name of [
      "seo_video_media",
      "seo_video_poster",
      "seo_video_title",
      "seo_video_description",
    ])
      form.elements.namedItem(name).value = "";
    form.elements.namedItem("seo_video_sitemap").checked = false;
    update();
    form.dispatchEvent(new Event("input", { bubbles: true }));
  });
  for (const control of form.querySelectorAll("[data-discovery-media]")) {
    const field = form.elements.namedItem(control.dataset.discoveryMedia);
    const choose = control.querySelector("[data-media-choose]");
    const options = control.querySelector("[data-media-options]");
    const status = control.querySelector("[data-media-status]");
    options.setAttribute("role", "group");
    options.setAttribute("aria-label", "Local media choices");
    choose.setAttribute("aria-expanded", "false");
    let loaded = 0;
    async function load(after = "") {
      choose.disabled = true;
      options.setAttribute("aria-busy", "true");
      status.textContent = "Loading local media…";
      try {
        const params = new URLSearchParams({
          media_kind: control.dataset.mediaKind,
          after,
        });
        const response = await fetch("/api/admin/media?" + params, {
          cache: "no-store",
        });
        if (!response.ok) throw Error();
        const data = await response.json();
        if (!control.isConnected) return;
        for (const item of data.items) {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "secondary";
          button.textContent = item.name + " · " + item.visibility;
          button.addEventListener("click", () => {
            field.value = item.id;
            field.dispatchEvent(new Event("input", { bubbles: true }));
            options.hidden = true;
            choose.setAttribute("aria-expanded", "false");
            choose.focus();
            status.textContent =
              "Selected " +
              item.name +
              ". Current public access controls live output.";
          });
          options.append(button);
        }
        loaded += data.items.length;
        if (data.next && loaded < 200) {
          const more = document.createElement("button");
          more.type = "button";
          more.className = "secondary";
          more.textContent = "More local media";
          more.addEventListener("click", () => {
            more.remove();
            load(data.next);
          });
          options.append(more);
        }
        status.textContent = data.items.length
          ? loaded >= 200
            ? "Showing 200 choices. Use Media library for another identity."
            : "Choose an asset; private assets stay protected."
          : "No more assets. Upload in Media library.";
      } catch {
        status.textContent =
          "Media list unavailable. Reopen to retry, or enter a known local UUID.";
      } finally {
        choose.disabled = false;
        options.removeAttribute("aria-busy");
      }
    }
    choose.addEventListener("click", () => {
      options.hidden = !options.hidden;
      choose.setAttribute("aria-expanded", String(!options.hidden));
      if (!options.hidden && !loaded) load();
    });
    control
      .querySelector("[data-media-clear]")
      .addEventListener("click", () => {
        field.value = "";
        field.dispatchEvent(new Event("input", { bubbles: true }));
        status.textContent = "Selection cleared.";
      });
  }
}
