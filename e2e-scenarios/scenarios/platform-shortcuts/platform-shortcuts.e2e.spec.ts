import { Key } from "webdriverio";

import { createFreshVault } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import { fixtures } from "../../support/fixtures";
import { pressPrimaryShortcut } from "../../support/keyboard";

async function activeTabIndex(): Promise<number> {
  const sessionTabs = await $$("[role='tablist'][aria-label='Session tabs'] [role='tab']").getElements();
  for (let index = 0; index < sessionTabs.length; index += 1) {
    const tab = sessionTabs[index];
    if ((await tab.getAttribute("aria-selected")) === "true") return index;
  }
  return -1;
}

async function waitForTabCount(count: number): Promise<void> {
  await browser.waitUntil(async () => (await $$("[role='tablist'][aria-label='Session tabs'] [role='tab']").getElements()).length === count, {
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
    await $("aria/Local pane actions").click();
    const uploadAction = await $("aria/Upload →");
    await uploadAction.waitForDisplayed({
      timeoutMsg: "The visible Actions button did not open the selected file's menu.",
    });
    await browser.keys(Key.Escape);
    await uploadAction.waitForDisplayed({ reverse: true });

    // Closing the actions menu returns focus to its button, so a user must
    // activate the file pane again before using its selection shortcut.
    await localOptions[0].click();
    await pressPrimaryShortcut("a");
    await browser.waitUntil(
      async () => {
        for (const option of localOptions) {
          if ((await option.getAttribute("aria-selected")) !== "true") return false;
        }
        return true;
      },
      { timeoutMsg: "The primary-modifier Select all shortcut did not select every local file." },
    );

    await pressPrimaryShortcut("t");
    await waitForTabCount(2);
    await waitForActiveTab(1);
    await waitForVisibleConnectionState("connected");

    await pressPrimaryShortcut("1");
    await waitForActiveTab(0);
    await waitForVisibleConnectionState("connected");

    await pressPrimaryShortcut("2");
    await waitForActiveTab(1);

    await pressPrimaryShortcut(",");
    const settings = await $("[role='dialog'][aria-label='Settings']");
    await settings.waitForDisplayed();
    await browser.keys(Key.Escape);
    await settings.waitForDisplayed({ reverse: true });

    await pressPrimaryShortcut("w");
    await waitForTabCount(1);
    await waitForActiveTab(0);
    await waitForVisibleConnectionState("connected");
  });
});
