#!/usr/bin/env node

import { readFileSync, readdirSync } from "node:fs";
import { pathToFileURL } from "node:url";

const assetDir = new URL("../apps/owner-tauri/dist/assets/", import.meta.url);
const asset = readdirSync(assetDir).find((name) => /^index-.*\.js$/.test(name));
if (!asset) {
  throw new Error("build apps/owner-tauri before running owner-tauri-smoke");
}
const sourceText = readFileSync(new URL("../apps/owner-tauri/src/main.ts", import.meta.url), "utf8");
const styleText = readFileSync(new URL("../apps/owner-tauri/src/styles.css", import.meta.url), "utf8");

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

function assertIncludes(value, expected, context) {
  if (!value.includes(expected)) {
    throw new Error(`${context} missing ${JSON.stringify(expected)}`);
  }
}

function assertMatches(value, pattern, context) {
  if (!pattern.test(value)) {
    throw new Error(`${context} missing pattern ${pattern}`);
  }
}

function assertNotMatches(value, pattern, context) {
  if (pattern.test(value)) {
    throw new Error(`${context} must not match ${pattern}`);
  }
}

function cssHexVar(name) {
  const match = styleText.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`));
  if (!match) {
    throw new Error(`styles.css missing CSS variable --${name}`);
  }
  return match[1];
}

function srgbChannel(value) {
  const normalized = value / 255;
  return normalized <= 0.03928
    ? normalized / 12.92
    : ((normalized + 0.055) / 1.055) ** 2.4;
}

function luminance(hex) {
  const value = hex.replace("#", "");
  const red = parseInt(value.slice(0, 2), 16);
  const green = parseInt(value.slice(2, 4), 16);
  const blue = parseInt(value.slice(4, 6), 16);
  return 0.2126 * srgbChannel(red) + 0.7152 * srgbChannel(green) + 0.0722 * srgbChannel(blue);
}

function contrastRatio(foreground, background) {
  const light = Math.max(luminance(foreground), luminance(background));
  const dark = Math.min(luminance(foreground), luminance(background));
  return (light + 0.05) / (dark + 0.05);
}

function assertContrast(label, foreground, background, minimum) {
  const ratio = contrastRatio(foreground, background);
  if (ratio < minimum) {
    throw new Error(`${label} contrast ${ratio.toFixed(2)} is below ${minimum}`);
  }
}

function runStaticReleaseGates() {
  assertIncludes(styleText, ":focus-visible", "visible focus CSS");
  assertIncludes(styleText, "@media (prefers-color-scheme: dark)", "dark-mode CSS");
  assertIncludes(styleText, "@media (max-width: 560px)", "narrow text-scaling CSS");
  assertNotMatches(styleText, /font(?:-size)?:[^;]*\bvw\b/i, "font sizing");
  assertContrast("primary text on surface", cssHexVar("text"), cssHexVar("surface"), 4.5);
  assertContrast("secondary text on surface", cssHexVar("secondary"), cssHexVar("surface"), 4.5);

  assertIncludes(sourceText, 'aria-label="Active Dais account"', "screen-reader labels");
  assertIncludes(sourceText, '<button type="button" data-section', "keyboard-operable navigation");
  assertIncludes(sourceText, "Public is internet-visible. Followers goes to approved followers. Direct is for named recipients only.", "compose privacy explainer");
  assertIncludes(sourceText, "Posting as", "compose identity selector");
  assertIncludes(sourceText, "Who can see this?", "compose audience selector");
  assertIncludes(sourceText, "Post Publicly", "public submit label");
  assertIncludes(sourceText, "Send Encrypted DM", "encrypted direct submit label");
  assertIncludes(sourceText, "Public posts are visible on the open web and public feeds.", "public-post warning");
  assertIncludes(sourceText, "Followers-only posts reach", "followers-only routing preview");
  assertIncludes(sourceText, "Direct posts need at least one named recipient.", "direct-recipient validation");
  assertIncludes(sourceText, "Private and direct posts need media uploaded while that visibility is selected.", "private media visibility guard");
  assertMatches(
    sourceText,
    /const access = visibility === "Followers" \|\| visibility === "Direct" \? "private" : "public";/,
    "media access routing"
  );
  assertIncludes(sourceText, "Switching changes which Dais instance receives reads, posts, replies, follows, watches, moderation, and operator commands.", "account-switching privacy note");
  console.log("PASS static release gates");
}

const screenChecks = [
  {
    mode: "Home",
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
    mode: "Server",
    section: "Profile",
    expected: [
      "Dais Smoke Account",
      "@social@dais.social",
      "Private-by-default social server smoke profile.",
      "Actor type",
    ],
  },
  {
    mode: "Home",
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
    mode: "Home",
    section: "Compose",
    expected: [
      "New post",
      "Posting as",
      "Who can see this?",
      "Public internet",
      "Direct / E2EE",
      "Post to Followers",
      "social.dais.social/media/smoke-upload.png",
      "Revoke upload",
      "Audience preview",
      "Followers-only posts reach 1 approved follower.",
      "Direct/E2EE actor URLs",
      "No obvious sensitive content",
      "No routing or sensitivity warnings detected for this draft.",
      "Approved followers",
    ],
  },
  {
    mode: "People",
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
    mode: "People",
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
    mode: "People",
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
    mode: "People",
    section: "Discovery",
    expected: [
      "Find actor",
      "@user@example.social or https://...",
      "Lookup",
      "Actor preview",
    ],
  },
  {
    mode: "People",
    section: "Followers",
    expected: [
      "Follower lists are owner-token views.",
      "Dais does not advertise them publicly by default.",
      "Pending",
      "Approved",
      "https://mastodon.example/users/alice",
    ],
  },
  {
    mode: "Server",
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
    mode: "Server",
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
  {
    mode: "Server",
    section: "Diagnostics",
    expected: [
      "owner-api",
      "Smoke fixture owner API",
    ],
  },
];

runStaticReleaseGates();

const coveredModes = new Set();
const coveredSections = new Set();

for (const check of screenChecks) {
  app.innerHTML = "";
  globalThis.window = {
    location: { search: `?smoke=1&section=${encodeURIComponent(check.section)}` },
    open() {},
  };
  await import(`${bundleUrl}?smoke-check=${check.section}-${Date.now()}`);
  await new Promise((resolve) => setTimeout(resolve, 25));

  for (const text of check.expected) {
    assertIncludes(app.innerHTML, text, `${check.section} smoke screen`);
  }
  assertIncludes(app.innerHTML, 'aria-label="Active Dais account"', `${check.section} screen-reader account switcher`);
  coveredModes.add(check.mode);
  coveredSections.add(check.section);
  console.log(`PASS ${check.mode}/${check.section}`);
}

for (const mode of ["Home", "People", "Server"]) {
  if (!coveredModes.has(mode)) {
    throw new Error(`smoke coverage missing IA mode ${mode}`);
  }
}

for (const section of ["Home", "Compose", "Settings", "Discovery", "Moderation"]) {
  if (!coveredSections.has(section)) {
    throw new Error(`smoke coverage missing required section ${section}`);
  }
}

console.log("PASS release gate coverage: Home, People, Server, Compose, Settings, Discovery, Moderation");
