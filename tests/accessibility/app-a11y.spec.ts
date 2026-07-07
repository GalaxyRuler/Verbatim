import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { installA11yTauriMocks } from "./tauriA11yMocks";

const expectNoAxeViolations = async (page: Page, surface: string) => {
  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
    .analyze();

  expect(
    results.violations.map((violation) => ({
      id: violation.id,
      impact: violation.impact,
      surface,
      description: violation.description,
      nodes: violation.nodes.map((node) => ({
        target: node.target,
        html: node.html,
        failureSummary: node.failureSummary,
      })),
    })),
  ).toEqual([]);
};

test.describe("accessibility gates", () => {
  for (const [language, sidebarLabel] of [
    ["ar", "عام"],
    ["he", "כללי"],
  ]) {
    test(`settings shell supports ${language} RTL mixed-direction layout`, async ({
      page,
    }) => {
      await installA11yTauriMocks(page, {
        settingsOverrides: { app_language: language },
      });
      await page.goto("/");

      await expect
        .poll(() =>
          page.evaluate(() => ({
            dir: document.documentElement.getAttribute("dir"),
            lang: document.documentElement.getAttribute("lang"),
          })),
        )
        .toEqual({ dir: "rtl", lang: language });
      await expect(
        page.getByRole("button", { name: sidebarLabel, exact: true }),
      ).toBeVisible();
      await expect
        .poll(() =>
          page.evaluate(
            () =>
              document.documentElement.scrollWidth <=
              document.documentElement.clientWidth,
          ),
        )
        .toBe(true);
    });
  }

  test("onboarding permissions view has no axe violations", async ({
    page,
  }) => {
    await installA11yTauriMocks(page, { microphoneDenied: true });
    await page.goto("/");

    await expect(
      page.getByRole("heading", { name: "Permissions required" }),
    ).toBeVisible();
    await expectNoAxeViolations(page, "onboarding permissions");
  });

  test("settings, model selector, post-processing, diagnostics, and history have no axe violations", async ({
    page,
  }) => {
    await installA11yTauriMocks(page);
    await page.goto("/");
    await expect(page.getByRole("button", { name: "General" })).toBeVisible();

    for (const sectionName of [
      "General",
      "Models",
      "History & Privacy",
      "Post-processing",
      "Troubleshooting",
    ]) {
      const sectionButton = page.getByRole("button", {
        name: sectionName,
        exact: true,
      });
      await sectionButton.press("Enter");
      await expect(sectionButton).toHaveAttribute("aria-current", "page");
      await expectNoAxeViolations(page, sectionName);
    }
  });

  test("recording overlay has no axe violations", async ({ page }) => {
    await installA11yTauriMocks(page);
    await page.goto("/src/overlay/index.html");

    await page.evaluate(() => {
      const win = window as typeof window & {
        __VERBATIM_A11Y_EMIT_EVENT__: (
          event: string,
          payload?: unknown,
        ) => void;
      };
      win.__VERBATIM_A11Y_EMIT_EVENT__("show-overlay", "recording");
    });

    await expect(page.getByRole("status")).toHaveAccessibleName("Recording");
    await expect
      .poll(() =>
        page
          .getByTestId("recording-overlay")
          .evaluate((element) => getComputedStyle(element).opacity),
      )
      .toBe("1");
    await expectNoAxeViolations(page, "recording overlay");
  });
});
