import assert from "node:assert/strict";

import { createFreshVault } from "../../support/app";
import { createConnection, openConnection } from "../../support/connections";
import { fixtures } from "../../support/fixtures";

const connectedState = "[data-testid='session-state'][data-state='connected']";
const remotePath = "[data-pane='remote'] button[aria-label$=' path']";

async function displayedElement(
  selector: string,
  timeoutMessage: string,
): Promise<WebdriverIO.Element> {
  let displayed: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      for (const element of await $$(selector).getElements()) {
        if (await element.isDisplayed()) {
          displayed = element;
          return true;
        }
      }
      return false;
    },
    { timeout: 30_000, timeoutMsg: timeoutMessage },
  );
  if (!displayed) throw new Error(timeoutMessage);
  return displayed;
}

async function sessionTabs(): Promise<WebdriverIO.Element[]> {
  return (await $$(
    "[role='tablist'][aria-label='Session tabs'] [role='tab']",
  ).getElements()) as unknown as WebdriverIO.Element[];
}

async function activeTabIndex(): Promise<number> {
  const tabs = await sessionTabs();
  for (let index = 0; index < tabs.length; index += 1) {
    if ((await tabs[index].getAttribute("aria-selected")) === "true") return index;
  }
  return -1;
}

async function waitForTabCount(count: number): Promise<void> {
  await browser.waitUntil(async () => (await sessionTabs()).length === count, {
    timeoutMsg: `Expected exactly ${count} session tab(s).`,
  });
}

async function activateTab(index: number): Promise<void> {
  const tabs = await sessionTabs();
  if (!tabs[index]) throw new Error(`Session tab ${index + 1} does not exist.`);
  await tabs[index].click();
  await browser.waitUntil(async () => (await activeTabIndex()) === index, {
    timeoutMsg: `Session tab ${index + 1} did not become active.`,
  });
}

async function waitForActiveConnection(): Promise<void> {
  await displayedElement(connectedState, "The active FTP tab did not become connected.");
}

async function activeRemotePath(): Promise<string> {
  return (await displayedElement(remotePath, "The active remote path was not visible.")).getText();
}

async function openRemoteDirectory(
  name: string,
  path: string,
  expectedEntry: string,
): Promise<void> {
  const directory = await displayedElement(
    `[data-pane='remote'] [role='option'][aria-label='${name}']`,
    `${name} was not visible in the active FTP tab.`,
  );
  await directory.click();
  await (
    await displayedElement(
      "[data-pane='remote'] [aria-label='Remote pane actions']",
      "The active remote pane actions were not visible.",
    )
  ).click();
  await (await displayedElement("aria/Open", "The visible pane menu did not expose Open.")).click();
  await browser.waitUntil(async () => (await activeRemotePath()) === path, {
    timeoutMsg: `The active FTP tab did not navigate to ${path}.`,
  });
  await displayedElement(
    `[data-pane='remote'] [role='option'][aria-label='${expectedEntry}']`,
    `${expectedEntry} was not visible after navigating to ${path}.`,
  );
}

async function closeActiveTab(): Promise<void> {
  for (const tab of await sessionTabs()) {
    if ((await tab.getAttribute("aria-selected")) === "true") {
      await tab.$("button[aria-label='Close tab']").click();
      return;
    }
  }
  throw new Error("No active session tab exists.");
}

describe("@ftp-tab-isolation", () => {
  it("keeps two tabs for one saved FTP connection independent", async () => {
    const fixture = fixtures();
    await createFreshVault("ftp-tab-isolation");
    await createConnection({
      name: "Tab Isolation FTP",
      protocol: "ftp",
      host: fixture.ftp.host,
      port: fixture.ftp.port,
      username: fixture.ftp.username,
      localDir: fixture.paths.local_source,
    });

    await openConnection("Tab Isolation FTP");
    await waitForTabCount(1);
    await waitForActiveConnection();
    await openRemoteDirectory("serverus-e2e", "/serverus-e2e", "site");
    await openRemoteDirectory("site", "/serverus-e2e/site", "index.html");

    await openConnection("Tab Isolation FTP");
    await waitForTabCount(2);
    assert.equal(await activeTabIndex(), 1);
    await waitForActiveConnection();
    assert.equal(await activeRemotePath(), "/");
    await displayedElement(
      "[data-pane='remote'] [role='option'][aria-label='serverus-e2e']",
      "The second FTP tab did not show the remote root.",
    );

    await activateTab(0);
    await waitForActiveConnection();
    assert.equal(await activeRemotePath(), "/serverus-e2e/site");
    await displayedElement(
      "[data-pane='remote'] [role='option'][aria-label='index.html']",
      "The first FTP tab lost its independent remote directory.",
    );

    await closeActiveTab();
    await waitForTabCount(1);
    assert.equal(await activeTabIndex(), 0);
    await waitForActiveConnection();
    assert.equal(await activeRemotePath(), "/");
    await openRemoteDirectory("serverus-e2e", "/serverus-e2e", "site");
    await openRemoteDirectory("site", "/serverus-e2e/site", "nested");
    await openRemoteDirectory("nested", "/serverus-e2e/site/nested", "readme.txt");
  });
});
