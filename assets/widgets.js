"use strict";
// Progressive tabs: without scripting every section remains readable.
for (const group of document.querySelectorAll("[data-tabs]")) {
  const list = group.querySelector(":scope > [data-tab-list]");
  const tabs = [...list.querySelectorAll("[data-tab]")];
  const panels = [...group.querySelectorAll(":scope > [data-tab-panel]")];
  if (!tabs.length) continue;
  list.setAttribute("role", "tablist");
  const activate = (index) => {
    tabs.forEach((tab, i) => {
      tab.setAttribute("role", "tab");
      tab.setAttribute("aria-selected", String(i === index));
      tab.tabIndex = i === index ? 0 : -1;
    });
    panels.forEach((panel, i) => {
      panel.setAttribute("role", "tabpanel");
      panel.hidden = i !== index;
    });
  };
  tabs.forEach((tab, i) => {
    tab.addEventListener("click", () => activate(i));
    tab.addEventListener("keydown", (event) => {
      let next;
      if (event.key === "ArrowRight") next = (i + 1) % tabs.length;
      else if (event.key === "ArrowLeft")
        next = (i + tabs.length - 1) % tabs.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = tabs.length - 1;
      else return;
      event.preventDefault();
      activate(next);
      tabs[next].focus();
    });
  });
  activate(0);
}
// Ordinary navigation keeps native lists/disclosures. Escape closes the nearest
// open group and returns focus to its summary; Tab remains normal document flow.
for (const navigation of document.querySelectorAll(".theme-navigation")) {
  navigation.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    const group = event.target.closest("details[open]");
    if (!group || !navigation.contains(group)) return;
    event.preventDefault();
    event.stopPropagation();
    for (const nested of group.querySelectorAll("details[open]")) nested.open = false;
    group.open = false;
    group.querySelector(":scope > summary")?.focus();
  });
}
