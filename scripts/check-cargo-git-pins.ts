import { readFileSync } from "node:fs";

const cargoToml = readFileSync("src-tauri/Cargo.toml", "utf8");
const failures: string[] = [];
const gitDependencyPattern = /^([^#\n]*git\s*=\s*"[^"]+"[^#\n]*)$/gm;

for (const match of cargoToml.matchAll(gitDependencyPattern)) {
  const line = match[1].trim();
  const hasImmutableRev = /\brev\s*=\s*"[0-9a-f]{40}"/i.test(line);
  const hasMovingSelector = /\b(branch|tag)\s*=/.test(line);

  if (!hasImmutableRev) {
    failures.push(`Git dependency must include a 40-character rev: ${line}`);
  }

  if (hasMovingSelector) {
    failures.push(`Git dependency must not use branch/tag selectors: ${line}`);
  }
}

if (failures.length > 0) {
  console.error("Cargo Git dependency pin check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log("Cargo Git dependency pin check passed.");
