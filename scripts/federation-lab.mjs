#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const defaultProfile = "docs/reference/federation-lab-targets.json";
const args = new Set(process.argv.slice(2));
const profilePath = valueAfter("--profile") || process.env.DAIS_FEDERATION_LAB_PROFILE || defaultProfile;
const outputJson = args.has("--json");

const requiredServers = ["mastodon", "pleroma", "misskey", "pixelfed"];
const requiredCapabilities = [
  "webfinger",
  "actor",
  "follow",
  "accept",
  "create",
  "reply",
  "like",
  "announce",
  "authorized_fetch",
  "private_visibility",
];

const capabilityLabels = {
  webfinger: "WebFinger discovery",
  actor: "Actor document",
  follow: "Follow request delivery",
  accept: "Accept delivery",
  create: "Create/Note delivery",
  reply: "Reply ingestion",
  like: "Like/Favourite ingestion",
  announce: "Announce/Boost ingestion",
  authorized_fetch: "Authorized fetch",
  private_visibility: "Followers-only/private visibility",
};

function valueAfter(name) {
  const index = process.argv.indexOf(name);
  if (index === -1) return "";
  return process.argv[index + 1] || "";
}

function readProfile(filePath) {
  const fullPath = path.resolve(process.cwd(), filePath);
  const text = fs.readFileSync(fullPath, "utf8");
  return JSON.parse(text);
}

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function statusFor(target, capability) {
  return target.capabilities?.[capability]?.status || "missing";
}

function detailFor(target, capability) {
  return target.capabilities?.[capability]?.detail || "";
}

function normalizeStatus(value) {
  if (value === "pass" || value === "manual" || value === "blocked" || value === "missing") {
    return value;
  }
  return "missing";
}

function row(server, target, capability, status, detail = "") {
  return {
    server,
    target,
    capability,
    label: capabilityLabels[capability] || capability,
    status,
    detail,
  };
}

function validate(profile) {
  const rows = [];
  const targets = asArray(profile.targets);
  const byServer = new Map(targets.map((target) => [target.server, target]));

  for (const server of requiredServers) {
    const target = byServer.get(server);
    if (!target) {
      for (const capability of requiredCapabilities) {
        rows.push(row(server, "", capability, "missing", "server profile is not configured"));
      }
      continue;
    }

    for (const capability of requiredCapabilities) {
      const status = normalizeStatus(statusFor(target, capability));
      rows.push(row(server, target.name || server, capability, status, detailFor(target, capability)));
    }
  }

  return rows;
}

function printMarkdown(rows) {
  console.log("| Server | Target | Capability | Status | Detail |");
  console.log("| --- | --- | --- | --- | --- |");
  for (const item of rows) {
    console.log(
      `| ${escapeCell(item.server)} | ${escapeCell(item.target)} | ${escapeCell(item.label)} | ${item.status.toUpperCase()} | ${escapeCell(item.detail)} |`,
    );
  }
}

function escapeCell(value) {
  return String(value ?? "").replaceAll("|", "\\|").replaceAll("\n", " ");
}

const profile = readProfile(profilePath);
const rows = validate(profile);
const missing = rows.filter((item) => item.status === "missing");
const blocked = rows.filter((item) => item.status === "blocked");
const manual = rows.filter((item) => item.status === "manual");
const pass = rows.filter((item) => item.status === "pass");

if (outputJson) {
  console.log(JSON.stringify({ profile: profile.name, rows }, null, 2));
} else {
  printMarkdown(rows);
}

console.error(
  `\nFederation lab: PASS=${pass.length} MANUAL=${manual.length} BLOCKED=${blocked.length} MISSING=${missing.length}`,
);

process.exit(missing.length > 0 ? 1 : 0);
