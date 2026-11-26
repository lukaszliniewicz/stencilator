import { defineConfig, devices } from '@playwright/test';

const PORT = 4173;

export default defineConfig({
  testDir: 'tests',
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? 'github' : 'list',
  use: { baseURL: `http://localhost:${String(PORT)}` },
  projects: [{ name: 'chromium', use: devices['Desktop Chrome'] }],
  // The production build, so the content security policy is in force.
  webServer: {
    command: `pnpm build && pnpm preview --port ${String(PORT)} --strictPort`,
    port: PORT,
    reuseExistingServer: !process.env.CI
  }
});
