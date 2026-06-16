#!/usr/bin/env node

import { readdirSync } from "node:fs";
import { pathToFileURL } from "node:url";

const assetDir = new URL("../apps/owner-tauri/dist/assets/", import.meta.url);
const asset = readdirSync(assetDir).find((name) => /^index-.*\.js$/.test(name));
if (!asset) {
  throw new Error("build apps/owner-tauri before running owner-tauri-smoke");
}

class AppElement {
  innerHTML = "";

  querySelector() {
    return null;
  }

  querySelectorAll() {
    return [];
  }

  addEventListener() {}
}

const app = new AppElement();

globalThis.document = {
  createElement(tagName) {
    return {
      tagName: String(tagName).toUpperCase(),
      relList: {
        supports() {
          return true;
        },
      },
    };
  },
  querySelector(selector) {
    return selector === "#app" ? app : null;
  },
  querySelectorAll() {
    return [];
  },
};
globalThis.MutationObserver = class {
  observe() {}
};
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: {
    clipboard: {
      async writeText() {},
    },
  },
});
globalThis.btoa = (value) => Buffer.from(value, "binary").toString("base64");

const bundleUrl = pathToFileURL(new URL(asset, assetDir).pathname).href;

for (const check of [
  {
    section: "Home",
    expected: [
      "Following feed",
      "A followed public post with reply, like, and boost actions.",
      "Private default",
      "Reply",
      "Boost",
    ],
  },
  {
    section: "Profile",
    expected: [
      "Dais Smoke Account",
      "@social@dais.social",
      "Private-by-default social server smoke profile.",
      "Actor type",
    ],
  },
  {
    section: "Posts",
    expected: [
      "Smoke public post",
      "Dais Desk smoke post detail content.",
      "Smoke reply rendered in post detail.",
      "Likes",
      "Boosts",
      "Open original",
    ],
  },
]) {
  app.innerHTML = "";
  globalThis.window = {
    location: { search: `?smoke=1&section=${encodeURIComponent(check.section)}` },
    open() {},
  };
  await import(`${bundleUrl}?smoke-check=${check.section}-${Date.now()}`);
  await new Promise((resolve) => setTimeout(resolve, 25));

  for (const text of check.expected) {
    if (!app.innerHTML.includes(text)) {
      throw new Error(`${check.section} smoke screen missing ${JSON.stringify(text)}`);
    }
  }
  console.log(`PASS ${check.section}`);
}
