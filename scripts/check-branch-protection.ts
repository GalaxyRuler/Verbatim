import { execFileSync } from "node:child_process";

const args = process.argv.slice(2);
const repo = argValue("--repo") ?? "GalaxyRuler/Verbatim";
const branch = argValue("--branch") ?? "main";
const requiredContexts = [
  "Windows x64 production backend",
  "macOS ARM64 production backend",
  "Ubuntu x64 production backend",
];

type BranchProtection = {
  required_status_checks?: {
    contexts?: string[];
  } | null;
};

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function readBranchProtection(): BranchProtection {
  try {
    const output = execFileSync(
      "gh",
      ["api", `repos/${repo}/branches/${branch}/protection`],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    return JSON.parse(output) as BranchProtection;
  } catch (error) {
    const message = errorMessage(error);
    console.error(
      `Unable to read branch protection for ${repo}@${branch}. Ensure the branch is protected and gh is authenticated. ${message}`,
    );
    process.exit(1);
  }
}

function errorMessage(error: unknown): string {
  if (
    error &&
    typeof error === "object" &&
    "stderr" in error &&
    Buffer.isBuffer(error.stderr)
  ) {
    return error.stderr.toString("utf8").trim();
  }
  if (
    error &&
    typeof error === "object" &&
    "stderr" in error &&
    typeof error.stderr === "string"
  ) {
    return error.stderr.trim();
  }
  return error instanceof Error ? error.message : String(error);
}

const protection = readBranchProtection();
const contexts = protection.required_status_checks?.contexts ?? [];
const missing = requiredContexts.filter(
  (context) => !contexts.includes(context),
);

if (missing.length > 0) {
  console.error(
    `Branch protection for ${repo}@${branch} is missing required native backend status checks:`,
  );
  for (const context of missing) {
    console.error(`- ${context}`);
  }
  console.error(
    `Configured contexts: ${contexts.length > 0 ? contexts.join(", ") : "(none)"}`,
  );
  process.exit(1);
}

console.log(
  `Branch protection for ${repo}@${branch} requires all native backend status checks.`,
);
