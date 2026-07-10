import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const repoRoot = join(import.meta.dir, "..");

const rulesWithRequiredCiCheck = JSON.stringify([
  {
    type: "required_status_checks",
    parameters: {
      required_status_checks: [
        { context: "code-quality" },
        { context: "ci-required" },
      ],
    },
  },
]);

const rulesWithoutRequiredCiCheck = JSON.stringify([
  {
    type: "required_status_checks",
    parameters: {
      required_status_checks: [{ context: "code-quality" }],
    },
  },
]);

function runChecker(response: string) {
  const tempDir = mkdtempSync(join(tmpdir(), "branch-protection-check-"));
  const responsePath = join(tempDir, "response.json");
  const argsPath = join(tempDir, "gh-args.txt");
  const ghPath = join(tempDir, "gh.cmd");

  try {
    writeFileSync(responsePath, response);
    writeFileSync(
      ghPath,
      [
        "@echo off",
        'echo %* > "%FAKE_GH_ARGS_FILE%"',
        'type "%FAKE_GH_RESPONSE_FILE%"',
      ].join("\r\n"),
    );

    const result = Bun.spawnSync(
      ["bun", "scripts/check-branch-protection.ts"],
      {
        cwd: repoRoot,
        env: {
          ...process.env,
          PATH: `${tempDir};${process.env.PATH ?? ""}`,
          FAKE_GH_ARGS_FILE: argsPath,
          FAKE_GH_RESPONSE_FILE: responsePath,
        },
        stdout: "pipe",
        stderr: "pipe",
      },
    );

    return {
      exitCode: result.exitCode,
      stdout: new TextDecoder().decode(result.stdout),
      stderr: new TextDecoder().decode(result.stderr),
      ghArgs: readFileSync(argsPath, "utf8").trim(),
    };
  } finally {
    // The caller has already received all observable process output above.
    rmSync(tempDir, { recursive: true, force: true });
  }
}

describe("check-branch-protection", () => {
  test("requires ci-required from the branch rules API", () => {
    const result = runChecker(rulesWithRequiredCiCheck);

    expect(result.exitCode).toBe(0);
    expect(result.ghArgs).toBe(
      "api repos/GalaxyRuler/Verbatim/rules/branches/main",
    );
    expect(result.stdout).toContain("ci-required");
  });

  test("reports ci-required when the required-status-check rule omits it", () => {
    const result = runChecker(rulesWithoutRequiredCiCheck);

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("missing required status check");
    expect(result.stderr).toContain("ci-required");
    expect(result.stderr).toContain("code-quality");
  });
});
