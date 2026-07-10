import { execFileSync } from "node:child_process";

const args = process.argv.slice(2);
const repo = argValue("--repo") ?? "GalaxyRuler/Verbatim";
const branch = argValue("--branch") ?? "main";
const requiredContext = "ci-required";

type BranchRule = {
  type?: unknown;
  parameters?: {
    required_status_checks?: Array<{
      context?: unknown;
    }>;
  } | null;
};

function argValue(name: string): string | undefined {
  const index = args.indexOf(name);
  if (index >= 0) return args[index + 1];
  const prefix = `${name}=`;
  return args.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function readBranchRules(): BranchRule[] {
  try {
    const output = execFileSync(
      "gh",
      ["api", `repos/${repo}/rules/branches/${branch}`],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    const rules: unknown = JSON.parse(output);
    if (!Array.isArray(rules)) {
      throw new Error("GitHub returned an unexpected branch rules response.");
    }
    return rules as BranchRule[];
  } catch (error) {
    const message = errorMessage(error);
    console.error(
      `Unable to read branch rules for ${repo}@${branch}. Ensure branch rulesets are configured and gh is authenticated. ${message}`,
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

function requiredStatusCheckContexts(rules: BranchRule[]): string[] {
  return rules.flatMap((rule) => {
    if (rule.type !== "required_status_checks") return [];

    return (rule.parameters?.required_status_checks ?? []).flatMap(
      ({ context }) => (typeof context === "string" ? [context] : []),
    );
  });
}

const rules = readBranchRules();
const contexts = requiredStatusCheckContexts(rules);

if (!contexts.includes(requiredContext)) {
  console.error(
    `Branch rules for ${repo}@${branch} are missing required status check:`,
  );
  console.error(`- ${requiredContext}`);
  console.error(
    `Configured required-status-check contexts: ${contexts.length > 0 ? contexts.join(", ") : "(none)"}`,
  );
  process.exit(1);
}

console.log(`Branch rules for ${repo}@${branch} require ${requiredContext}.`);
