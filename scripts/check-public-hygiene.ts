// Public repository hygiene gate.
//
// Scans tracked text files for things that should never ship in a public repo:
//   1. The maintainer's private Gmail address (public contact must be
//      aoa@live.ca, never the personal Gmail).
//   2. Well-known secret/token formats (AWS keys, GitHub/Slack/Google tokens,
//      private-key blocks) — formats chosen for ~zero false-positive rate.
//   3. Private-network and dev-tunnel URLs left in source (10.x/192.168.x/172.16-31.x,
//      ngrok, trycloudflare). NOTE: localhost/127.0.0.1 are intentionally NOT
//      flagged — Verbatim is local-first and legitimately documents/uses local
//      endpoints (LM Studio, Ollama, asset.localhost).
//
// Exits non-zero with a report if any finding is detected.
//
// Usage:
//   bun scripts/check-public-hygiene.ts            # scan the published surface
//   bun scripts/check-public-hygiene.ts -- --all   # scan every tracked text file

import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

const scanAll = process.argv.slice(2).includes("--all");

// This script intentionally contains the detection patterns as source, so it
// must never scan itself.
const SELF = "scripts/check-public-hygiene.ts";

// Binary / generated / vendored paths that carry no human-authored secrets and
// would only add noise (or are huge).
const SKIP_EXACT = new Set([
  "bun.lock",
  "package-lock.json",
  "yarn.lock",
  "pnpm-lock.yaml",
  "src-tauri/Cargo.lock",
  ".nix/bun.nix",
  SELF,
]);

const SKIP_DIR_PREFIXES = [
  "node_modules/",
  "dist/",
  "src-tauri/target/",
  "src-tauri/gen/", // generated platform projects + icons
  ".git/",
];

const BINARY_EXT = new Set([
  ".png",
  ".jpg",
  ".jpeg",
  ".gif",
  ".webp",
  ".ico",
  ".icns",
  ".bmp",
  ".svg",
  ".woff",
  ".woff2",
  ".ttf",
  ".otf",
  ".eot",
  ".wasm",
  ".onnx",
  ".bin",
  ".gguf",
  ".safetensors",
  ".node",
  ".zip",
  ".gz",
  ".tar",
  ".7z",
  ".pdf",
  ".mp3",
  ".wav",
  ".ogg",
  ".flac",
  ".m4a",
  ".mp4",
  ".mov",
  ".keystore",
  ".jks",
  ".p12",
  ".der",
]);

// Without --all, only scan the public-facing surface to keep the default fast.
const PUBLISHED_PREFIXES = [
  "README",
  "docs/",
  "src/",
  "src-tauri/src/",
  "scripts/",
  ".github/",
  "package.json",
  "SECURITY.md",
  "CONTRIBUTING.md",
];

interface Rule {
  id: string;
  description: string;
  regex: RegExp;
  // Optional predicate to drop known-legit matches (false positives).
  allow?: (match: string, file: string, line: string) => boolean;
}

const RULES: Rule[] = [
  {
    id: "private-gmail",
    description:
      "Maintainer private Gmail address leaked (public contact must be aoa@live.ca).",
    // The personal Gmail local-part, in any dotting, at gmail.com.
    regex: /\ba[._]?o[._]?alkulaib@gmail\.com\b/gi,
  },
  {
    id: "gmail-contact",
    description:
      "A gmail.com address used as a contact in a public file (use aoa@live.ca).",
    regex: /[A-Za-z0-9._%+-]+@gmail\.com\b/gi,
  },
  {
    id: "aws-access-key",
    description: "AWS access key id.",
    regex: /\bAKIA[0-9A-Z]{16}\b/g,
  },
  {
    id: "github-token",
    description: "GitHub personal access / app token.",
    regex: /\bgh[pousr]_[A-Za-z0-9]{36,}\b/g,
  },
  {
    id: "slack-token",
    description: "Slack token.",
    regex: /\bxox[baprs]-[A-Za-z0-9-]{10,}\b/g,
  },
  {
    id: "google-api-key",
    description: "Google API key.",
    regex: /\bAIza[0-9A-Za-z_-]{35}\b/g,
  },
  {
    id: "private-key-block",
    description: "Private key block.",
    regex: /-----BEGIN (?:RSA |EC |OPENSSH |PGP |DSA )?PRIVATE KEY-----/g,
  },
  {
    id: "private-network-url",
    description:
      "Private-network / dev-tunnel host left in source (not localhost).",
    regex:
      /\b(?:https?:\/\/)?(?:(?:10|192\.168|172\.(?:1[6-9]|2\d|3[01]))(?:\.\d{1,3}){2}|[a-z0-9-]+\.ngrok(?:-free)?\.(?:io|app)|[a-z0-9-]+\.trycloudflare\.com)\b/gi,
  },
];

function listFiles(): string[] {
  const out = execSync("git ls-files", {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return out
    .split("\n")
    .map((f) => f.trim())
    .filter(Boolean)
    .map((f) => f.replace(/\\/g, "/"));
}

function shouldScan(file: string): boolean {
  if (SKIP_EXACT.has(file)) return false;
  if (SKIP_DIR_PREFIXES.some((p) => file.startsWith(p))) return false;
  if (BINARY_EXT.has(path.extname(file).toLowerCase())) return false;
  if (!scanAll && !PUBLISHED_PREFIXES.some((p) => file.startsWith(p))) {
    return false;
  }
  return true;
}

interface Finding {
  rule: Rule;
  file: string;
  lineNo: number;
  line: string;
  match: string;
}

const findings: Finding[] = [];

for (const file of listFiles()) {
  if (!shouldScan(file)) continue;

  let content: string;
  try {
    content = readFileSync(file, "utf8");
  } catch {
    continue; // unreadable / binary slipped through
  }
  // Skip files that look binary (NUL byte present).
  if (content.includes("\u0000")) continue;

  const lines = content.split(/\r?\n/);
  for (const rule of RULES) {
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      rule.regex.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = rule.regex.exec(line)) !== null) {
        const match = m[0];
        if (rule.allow && rule.allow(match, file, line)) continue;
        findings.push({ rule, file, lineNo: i + 1, line: line.trim(), match });
        if (m.index === rule.regex.lastIndex) rule.regex.lastIndex++;
      }
    }
  }
}

if (findings.length === 0) {
  console.log(
    `Public hygiene check passed (${scanAll ? "all tracked files" : "published surface"}).`,
  );
  process.exit(0);
}

console.error("Public hygiene check FAILED. Findings:\n");
const byRule = new Map<string, Finding[]>();
for (const f of findings) {
  const arr = byRule.get(f.rule.id) ?? [];
  arr.push(f);
  byRule.set(f.rule.id, arr);
}
for (const [ruleId, list] of byRule) {
  console.error(`[${ruleId}] ${list[0].rule.description}`);
  for (const f of list) {
    console.error(`  ${f.file}:${f.lineNo}: ${f.match}`);
  }
  console.error("");
}
console.error(
  `${findings.length} finding(s). Remove the leaked value(s) or, for an intentional/test value, scope the rule in ${SELF}.`,
);
process.exit(1);
