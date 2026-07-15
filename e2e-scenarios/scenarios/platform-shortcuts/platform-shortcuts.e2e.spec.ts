import { Key } from "webdriverio";

import { createFreshVault } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import { fixtures } from "../../support/fixtures";

const primaryModifier = process.platform === "darwin" ? Key.Command : Key.Control;

async function pressShortcut(key: string): Promise<void> {
  await browser
    .action("key")
    .down(primaryModifier)
    .down(key)
    .up(key)
    .up(primaryModifier)
    .perform();
}

async function activeTabIndex(): Promise<number> {
  const sessionTabs = await $$("[role='tab']").getElements();
  for (let index = 0; index < sessionTabs.length; index += 1) {
    const tab = sessionTabs[index];
    if ((await tab.getAttribute("aria-selected")) === "true") return index;
  }
  return -1;
}

async function waitForTabCount(count: number): Promise<void> {
  await browser.waitUntil(async () => (await $$("[role='tab']").getElements()).length === count, {
    timeoutMsg: `Expected ${count} visible session tab${count === 1 ? "" : "s"}.`,
  });
}

async function waitForActiveTab(index: number): Promise<void> {
  await browser.waitUntil(async () => (await activeTabIndex()) === index, {
    timeoutMsg: `Expected session tab ${index + 1} to be active.`,
  });
}

async function waitForVisibleConnectionState(state: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      const statusElements = await $$(
        `[data-testid='session-state'][data-state='${state}']`,
      ).getElements();
      for (const status of statusElements) {
        if (await status.isDisplayed()) return true;
      }
      return false;
    },
    { timeout: 30_000, timeoutMsg: `Active connection did not become ${state}.` },
  );
}

describe("@platform-shortcuts", () => {
  it("uses the host primary modifier for tab, Settings, and selection shortcuts", async () => {
    const fixture = fixtures();
    await createFreshVault("platform-shortcuts");
    await createConnection({
      name: "Shortcut FTP",
      protocol: "ftp",
      host: fixture.ftp.host,
      port: fixture.ftp.port,
      username: fixture.ftp.username,
      localDir: fixture.paths.local_source,
    });
    await openConnection("Shortcut FTP");
    await waitForConnected();
    await waitForTabCount(1);
    await waitForActiveTab(0);

    const localOptions = await $$("[data-pane='local'] [role='option']").getElements();
    if (localOptions.length === 0) throw new Error("The local fixture tree is empty.");
    await localOptions[0].click();
    if (process.platform === "win32") {
      await localOptions[0].click({ button: "right" });
      const uploadAction = await $("aria/Upload →");
      await uploadAction.waitForDisplayed({
        timeoutMsg: "A real WebView2 right click did not open the file context menu.",
      });
      await browser.keys(Key.Escape);
      await uploadAction.waitForDisplayed({ reverse: true });
    }

    await pressShortcut("a");
    await browser.waitUntil(
      async () => {
        for (const option of localOptions) {
          if ((await option.getAttribute("aria-selected")) !== "true") return false;
        }
        return true;
      },
      { timeoutMsg: "The primary-modifier Select all shortcut did not select every local file." },
    );

    await pressShortcut("t");
    await waitForTabCount(2);
    await waitForActiveTab(1);
    await waitForVisibleConnectionState("connected");

    await pressShortcut("1");
    await waitForActiveTab(0);
    await waitForVisibleConnectionState("connected");

    await pressShortcut("2");
    await waitForActiveTab(1);

    await pressShortcut(",");
    const settings = await $("[role='dialog'][aria-label='Settings']");
    await settings.waitForDisplayed();
    await browser.keys(Key.Escape);
    await settings.waitForDisplayed({ reverse: true });

    await pressShortcut("w");
    await waitForTabCount(1);
    await waitForActiveTab(0);
    await waitForVisibleConnectionState("connected");
  });
});
