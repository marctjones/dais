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
      "Delete post",
      "Revoke media",
    ],
  },
  {
    section: "Compose",
    expected: [
      "New post",
      "social.dais.social/media/smoke-upload.png",
      "Revoke upload",
      "Audience preview",
      "Followers-only posts reach 1 approved follower.",
      "No obvious sensitive content",
      "No routing or sensitivity warnings detected for this draft.",
      "Approved followers",
    ],
  },
  {
    section: "Audience",
    expected: [
      "Audience lists",
      "Close friends",
      "Small direct audience for sensitive updates.",
      "Edit list",
      "Allowed sensitive categories",
      "Members",
    ],
  },
  {
    section: "Watches",
    expected: [
      "Watches",
      "Private public-post monitoring without follows, approvals, or remote subscription records",
      "ActivityPub actor",
      "Bluesky actor",
      "NASA on Bluesky",
      "nasa.gov",
      "Harvested public posts",
      "Public launch update",
      "A public post harvested into the private watch reader.",
    ],
  },
  {
    section: "Search",
    expected: [
      "Search",
      "All providers",
      "Bluesky",
      "ActivityPub",
      "Posts + actors",
      "ActivityPub servers",
      "Public posts",
      "Public actors",
      "Providers",
    ],
  },
  {
    section: "Settings",
    expected: [
      "Accounts",
      "Dais Social",
      "Skeptical Engineer",
      "Jones Law",
      "Add or update account",
      "https://skeptical.engineer",
      "https://joneslaw.io",
    ],
  },
  {
    section: "Moderation",
    expected: [
      "Federation safety",
      "Reply policy: review",
      "Reply queue",
      "Workers AI live advisory mode",
      "This is a medical update reply that should stay in review.",
      "AI advisory (llama-guard-3-8b): Likely private medical content.",
      "Save policy",
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
