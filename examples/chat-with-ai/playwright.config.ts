import { defineConfig } from '@playwright/test';

const chatTestRoom = process.env.CHAT_TEST_ROOM ?? `playwright-room-${Date.now()}`;
const chatTestPort = Number(process.env.CHAT_TEST_PORT ?? 5174);
process.env.CHAT_TEST_ROOM = chatTestRoom;

export default defineConfig({
  testDir: './tests',
  testMatch: '**/*.spec.mjs',
  timeout: 90_000,
  fullyParallel: false,
  use: {
    baseURL: `http://127.0.0.1:${chatTestPort}`,
    headless: true,
  },
  webServer: {
    command: `npm run dev -- --host 127.0.0.1 --port ${chatTestPort} --strictPort`,
    env: {
      ...process.env,
      VITE_CHAT_ROOM: chatTestRoom,
    },
    port: chatTestPort,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});