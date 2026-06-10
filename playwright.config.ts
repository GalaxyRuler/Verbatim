import { defineConfig, devices } from "@playwright/test";

const chromiumOptions = process.env.PLAYWRIGHT_USE_SYSTEM_CHROME
  ? { ...devices["Desktop Chrome"], channel: "chrome" as const }
  : { ...devices["Desktop Chrome"] };

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  use: {
    baseURL: "http://localhost:1420",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: chromiumOptions,
    },
  ],
  webServer: {
    command: "bunx vite dev",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 30000,
  },
});
