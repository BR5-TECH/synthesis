// Records the addresses the app asked the platform to open, for Playwright to read.
(globalThis as Record<string, unknown>).__openedUrls = [];

export async function openUrl(url: string) {
  ((globalThis as Record<string, unknown>).__openedUrls as string[]).push(url);
}

export async function openPath() {}
export async function revealItemInDir() {}
