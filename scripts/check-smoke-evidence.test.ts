import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve, sep } from "node:path";

type Checker = "install" | "accessibility" | "updater";
type Fixture = Record<string, unknown>;

const repoRoot = resolve(import.meta.dir, "..");
const generatedDevVersion = "0.14.3-dev.202610091500+40554";
const requiredFields: Record<Checker, string[]> = {
  install: [
    "clean_machine",
    "install_verified",
    "launch_verified",
    "local_transcription_verified",
    "plain_text_insertion_verified",
    "uninstall_verified",
    "app_removed_after_uninstall",
    "app_data_policy_checked",
    "trust_behavior_matches_release_notes",
    "windows_default_uninstall_preserved_app_data",
    "windows_delete_app_data_removed_app_data",
  ],
  accessibility: [
    "onboarding_verified",
    "settings_navigation_verified",
    "recording_verified",
    "cancellation_verified",
    "paste_failure_recovery_verified",
    "history_review_verified",
    "keyboard_only_navigation_verified",
    "live_states_announced_without_transcript_leak",
    "rtl_mixed_direction_verified",
  ],
  updater: [
    "previous_install_verified",
    "update_detected",
    "update_downloaded",
    "updater_signature_verified",
    "update_applied",
    "clean_profile_preserved",
  ],
};

function fixture(
  checker: Checker,
  version: unknown = generatedDevVersion,
): Fixture {
  // These are disposable unit-test inputs, never real release certifications.
  const evidence: Fixture = {
    schema_version: 1,
    platform: checker === "accessibility" ? "windows" : "windows-x86_64",
    synthetic_unit_fixture: true,
    version,
    artifact_name: "SYNTHETIC-NO-INSTALLER",
    tester: "synthetic-unit-test",
    tested_at: "2026-10-09T00:00:00Z",
    assistive_technology: "NVDA",
    previous_version: "0.14.1",
    target_version: version,
    relaunched_version: version,
    latest_json_url:
      "https://example.invalid/SYNTHETIC-NOT-A-RELEASE/latest.json",
    updater_archive_name: "SYNTHETIC-NO-ARCHIVE",
    failures: [],
  };
  for (const field of requiredFields[checker]) evidence[field] = true;
  return evidence;
}

function runChecker(checker: Checker, records: Fixture[], args: string[] = []) {
  const tempRoot = resolve(tmpdir());
  const tempDir = mkdtempSync(join(tempRoot, "verbatim-evidence-unit-"));
  try {
    records.forEach((record, index) => {
      writeFileSync(
        join(tempDir, `${checker}-smoke-synthetic-unit-${index}.json`),
        JSON.stringify(record),
      );
    });
    const result = Bun.spawnSync(
      [
        process.execPath,
        join(repoRoot, "scripts", `check-${checker}-smoke-evidence.ts`),
        "--dir",
        tempDir,
        ...args,
      ],
      { cwd: repoRoot, stdout: "pipe", stderr: "pipe" },
    );
    return {
      exitCode: result.exitCode,
      stdout: new TextDecoder().decode(result.stdout),
      stderr: new TextDecoder().decode(result.stderr),
    };
  } finally {
    const target = resolve(tempDir);
    if (
      !target.startsWith(`${tempRoot}${sep}`) ||
      !basename(target).startsWith("verbatim-evidence-unit-")
    ) {
      throw new Error(
        `Refusing to remove unexpected fixture directory: ${target}`,
      );
    }
    rmSync(target, { recursive: true, force: true });
  }
}

for (const checker of ["install", "accessibility", "updater"] as const) {
  describe(`${checker} smoke evidence`, () => {
    test("accepts generated Dev versions with prerelease and build metadata", () => {
      const result = runChecker(checker, [fixture(checker)]);
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("check passed");
    });

    for (const version of [
      "0.14.2",
      "1.2.3-rc.1",
      "1.2.3+build.01",
      "1.2.3-alpha.1+build.99",
    ]) {
      test(`accepts SemVer ${version}`, () => {
        expect(runChecker(checker, [fixture(checker, version)]).exitCode).toBe(
          0,
        );
      });
    }

    for (const version of [
      undefined,
      null,
      123,
      "",
      "1.2",
      "v1.2.3",
      "01.2.3",
      "1.2.3-01",
      "1.2.3-beta..1",
      "1.2.3+build..2",
      "1.2.3-rc.1+build+again",
    ]) {
      test(`rejects malformed version ${String(version)}`, () => {
        const record = fixture(checker, version);
        // Passing undefined to a defaulted parameter does not create missing evidence.
        if (version === undefined) {
          delete record.version;
          delete record.target_version;
          delete record.relaunched_version;
        }
        const result = runChecker(checker, [record]);
        expect(result.exitCode).toBe(1);
        expect(result.stderr).toContain("semantic version");
      });
    }

    for (const field of requiredFields[checker]) {
      for (const value of [false, undefined]) {
        test(`requires ${field} when ${value === false ? "false" : "missing"}`, () => {
          const record = fixture(checker);
          if (value === undefined) delete record[field];
          else record[field] = value;
          const result = runChecker(checker, [record]);
          expect(result.exitCode).toBe(1);
          expect(result.stderr).toContain(`${field} must be true`);
        });
      }
    }

    test("rejects unsupported platforms and schema versions", () => {
      for (const change of [
        { platform: "unsupported" },
        { schema_version: 2 },
      ]) {
        expect(
          runChecker(checker, [{ ...fixture(checker), ...change }]).exitCode,
        ).toBe(1);
      }
    });

    test("rejects recorded failures", () => {
      const result = runChecker(checker, [
        { ...fixture(checker), failures: ["synthetic failure"] },
      ]);
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("recorded failure: synthetic failure");
    });

    test("requires all representative release platforms", () => {
      const result = runChecker(
        checker,
        [fixture(checker)],
        ["--require-representative-platforms"],
      );
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("Missing");
    });

    test("accepts complete representative platform evidence", () => {
      const platforms =
        checker === "accessibility"
          ? ["windows", "macos", "linux"]
          : ["windows-x86_64", "darwin-aarch64", "linux-x86_64"];
      const records = platforms.map((platform, index) => ({
        ...fixture(checker),
        platform,
        assistive_technology: ["NVDA", "VoiceOver", "Orca"][index],
        linux_package_manager_verified: true,
        macos_gatekeeper_verified: true,
      }));
      expect(
        runChecker(checker, records, ["--require-representative-platforms"])
          .exitCode,
      ).toBe(0);
    });
  });
}

describe("platform install trust", () => {
  test("requires macOS Gatekeeper evidence for signed installs", () => {
    const record = { ...fixture("install"), platform: "darwin-aarch64" };
    expect(runChecker("install", [record]).exitCode).toBe(0);
    const result = runChecker("install", [record], ["--signed"]);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("macos_gatekeeper_verified");
    expect(
      runChecker(
        "install",
        [{ ...record, macos_gatekeeper_verified: true }],
        ["--signed"],
      ).exitCode,
    ).toBe(0);
  });

  test("requires Linux package-manager evidence", () => {
    const record = {
      ...fixture("install"),
      platform: "linux-x86_64",
      linux_package_manager_verified: false,
    };
    const result = runChecker("install", [record]);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("linux_package_manager_verified");
  });

  test("requires an installer artifact name", () => {
    expect(
      runChecker("install", [{ ...fixture("install"), artifact_name: " " }])
        .exitCode,
    ).toBe(1);
  });
});

describe("updater release validation", () => {
  test("requires distinct previous and target versions", () => {
    const result = runChecker("updater", [
      { ...fixture("updater"), previous_version: generatedDevVersion },
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("must differ");
  });

  test("requires a valid previous version", () => {
    const result = runChecker("updater", [
      { ...fixture("updater"), previous_version: "1.2.3-01" },
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain(
      "previous_version must be a semantic version",
    );
  });

  test("requires relaunch into the target version", () => {
    const result = runChecker("updater", [
      { ...fixture("updater"), relaunched_version: "0.14.1" },
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("relaunched_version must match");
  });

  test("requires HTTPS metadata and an archive name", () => {
    for (const change of [
      { latest_json_url: "http://example.invalid/latest.json" },
      { latest_json_url: "bad-url" },
      { updater_archive_name: " " },
    ]) {
      expect(
        runChecker("updater", [{ ...fixture("updater"), ...change }]).exitCode,
      ).toBe(1);
    }
  });
});

describe("accessibility certification metadata", () => {
  test("requires the platform assistive technology", () => {
    const result = runChecker("accessibility", [
      { ...fixture("accessibility"), assistive_technology: "wrong" },
    ]);
    expect(result.exitCode).toBe(1);
    expect(result.stderr).toContain("assistive_technology must be NVDA");
  });

  test("requires tester identity and test date", () => {
    for (const change of [{ tester: " " }, { tested_at: "invalid-date" }]) {
      expect(
        runChecker("accessibility", [
          { ...fixture("accessibility"), ...change },
        ]).exitCode,
      ).toBe(1);
    }
  });
});
