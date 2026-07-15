import assert from "node:assert/strict";
import type { Socket } from "node:net";
import { join } from "node:path";

import { createFreshVault } from "../../support/app";
import { choosePaneAction } from "../../support/files";
import { fixtures } from "../../support/fixtures";
import { expectTerminalText } from "../../support/terminal";
import { unlockAfterAutoLock } from "./session-lifecycle-lock.support";
import {
  acceptUnknownHost,
  activateSessionTab,
  closeActiveSessionTab,
  configureOneMinuteAutoLock,
  configureScenarioEditor,
  createScenarioSshConnection,
  displayedElement,
  editCacheDirectories,
  openScenarioConnection,
  openTunnelProbe,
  reserveLocalPort,
  runTerminalMarker,
  sendTunnelRequest,
  waitForConnected,
  waitForEditCacheCleanup,
  waitForFilesystemPath,
  waitForNewEditCache,
  waitForSessionTabCount,
  waitForSocketClose,
  waitForTunnelListenerRelease,
} from "./session-lifecycle.support";

async function waitForTerminalCount(count: number): Promise<void> {
  await browser.waitUntil(
    async () => {
      const terminals = await $$(`button[aria-label^='Terminal ']`).getElements();
      const visibility = await terminals.map((terminal) => terminal.isDisplayed());
      return visibility.filter(Boolean).length === count;
    },
    { timeoutMsg: `Expected ${count} terminal channel(s).` },
  );
}

async function lifecycleTunnel(): Promise<WebdriverIO.Element> {
  return displayedElement("[role='group'][aria-label='Tunnel Scenario tunnel']");
}

async function tunnelButton(label: "Start" | "Stop"): Promise<WebdriverIO.Element> {
  const button = await (await lifecycleTunnel()).$(`button=${label}`);
  await button.waitForDisplayed();
  return button as unknown as WebdriverIO.Element;
}

async function tunnelCounter(title: "Uploaded" | "Downloaded"): Promise<string> {
  return (await (await lifecycleTunnel()).$(`[title='${title}']`)).getText();
}

describe("@session-lifecycle", () => {
  it("keeps sibling SSH resources isolated across auto-lock and tab cleanup", async function () {
    if (!fixtures().ssh.available) this.skip();

    const localPort = await reserveLocalPort();
    const baselineEdits = await editCacheDirectories();
    const probes: Socket[] = [];

    try {
      await createFreshVault("session-lifecycle");
      await configureScenarioEditor();
      await createScenarioSshConnection(localPort);

      await openScenarioConnection();
      await acceptUnknownHost();
      await waitForConnected();
      await displayedElement("button=Terminal").then((button) => button.click());
      await runTerminalMarker("TAB_A_BEFORE_LOCK");

      await openScenarioConnection();
      await waitForSessionTabCount(2);
      await waitForConnected();
      await displayedElement("button=Terminal").then((button) => button.click());
      await runTerminalMarker("TAB_B_TERMINAL_1");

      await displayedElement("aria/New terminal").then((button) => button.click());
      await displayedElement("aria/Terminal 2");
      await runTerminalMarker("TAB_B_TERMINAL_2");
      await displayedElement("aria/New terminal").then((button) => button.click());
      await displayedElement("aria/Terminal 3");
      await runTerminalMarker("TAB_B_TERMINAL_3");
      await displayedElement("aria/Terminal 1").then((button) => button.click());
      await runTerminalMarker("TAB_B_TERMINAL_1_STILL_RUNNING");
      await displayedElement("aria/Close terminal 2").then((button) => button.click());
      await waitForTerminalCount(2);
      await displayedElement("aria/Terminal 2").then((button) => button.click());
      await runTerminalMarker("TAB_B_TERMINAL_3_SURVIVED_REINDEX");

      await activateSessionTab(0);
      await runTerminalMarker("TAB_A_STILL_RUNNING");
      await displayedElement("button=Files").then((button) => button.click());
      const remoteFile = await displayedElement(
        "[data-pane='remote'] [role='option'][aria-label='index.html']",
      );
      await choosePaneAction("remote", remoteFile, "← Download");
      const transferSummary = await $("[data-testid='transfer-summary']");
      await transferSummary.waitUntil(
        async () =>
          (await transferSummary.getAttribute("data-done")) === "1" &&
          (await transferSummary.getAttribute("data-failed")) === "0",
        { timeout: 60_000, timeoutMsg: "The tab's SFTP download did not finish." },
      );
      assert.match(await transferSummary.getText(), /1 done/u);

      await choosePaneAction("remote", remoteFile, "Edit…");
      const createdEdits = await waitForNewEditCache(baselineEdits);

      await displayedElement("button=Tunnels").then((button) => button.click());
      await (await tunnelButton("Start")).click();
      await tunnelButton("Stop");
      assert.match(await (await lifecycleTunnel()).getText(), new RegExp(`localhost:${localPort}`));

      const liveProbe = await openTunnelProbe(localPort, "pre-lock");
      probes.push(liveProbe);
      await browser.waitUntil(async () => (await (await lifecycleTunnel()).getText()).includes("1 conn"), {
        timeout: 30_000,
        timeoutMsg: "Tunnel UI did not show its open connection.",
      });

      await displayedElement("button=Terminal").then((button) => button.click());
      await runTerminalMarker("TAB_A_IMMEDIATELY_BEFORE_AUTO_LOCK");
      await configureOneMinuteAutoLock();
      await unlockAfterAutoLock();

      await runTerminalMarker("TAB_A_AFTER_AUTO_LOCK");
      await expectTerminalText("TAB_A_BEFORE_LOCK_42");
      await displayedElement("button=Tunnels").then((button) => button.click());
      await tunnelButton("Stop");
      await browser.waitUntil(
        async () => (await (await lifecycleTunnel()).getText()).includes("1 conn"),
      );
      await sendTunnelRequest(liveProbe);
      assert.match(await tunnelCounter("Uploaded"), /^↑\s+\d+(?:\.\d+)?\s+[KMGT]?B$/u);
      assert.match(await tunnelCounter("Downloaded"), /^↓\s+\d+(?:\.\d+)?\s+[KMGT]?B$/u);
      await browser.waitUntil(async () => {
        const uploadedText = await tunnelCounter("Uploaded");
        const downloadedText = await tunnelCounter("Downloaded");
        return uploadedText !== "↑ 0 B" && downloadedText !== "↓ 0 B";
      }, { timeoutMsg: "Tunnel UI did not show forwarded traffic counters." });

      await (await tunnelButton("Stop")).click();
      await waitForSocketClose(liveProbe);
      await waitForTunnelListenerRelease(localPort);
      await (await tunnelButton("Start")).click();
      await tunnelButton("Stop");

      await displayedElement("button=Files").then((button) => button.click());
      const cleanupDownload = await displayedElement(
        "[data-pane='remote'] [role='option'][aria-label='cleanup-slow.bin']",
      );
      await choosePaneAction("remote", cleanupDownload, "← Download");
      await transferSummary.waitUntil(
        async () =>
          (await transferSummary.getAttribute("data-total")) === "2" &&
          (await transferSummary.getAttribute("data-running")) === "1" &&
          (await transferSummary.getAttribute("data-queued")) === "0" &&
          (await transferSummary.getAttribute("data-done")) === "1" &&
          (await transferSummary.getAttribute("data-failed")) === "0",
        { timeout: 30_000, timeoutMsg: "The deterministic cleanup download did not start." },
      );
      const cleanupLocalPath = join(fixtures().paths.local_download, "cleanup-slow.bin");
      await waitForFilesystemPath(cleanupLocalPath, true);

      const cleanupProbe = await openTunnelProbe(localPort, "cleanup");
      probes.push(cleanupProbe);
      await closeActiveSessionTab();
      await waitForSessionTabCount(1);
      await transferSummary.waitForExist({ reverse: true, timeout: 30_000 });
      await waitForEditCacheCleanup(createdEdits);
      await waitForSocketClose(cleanupProbe);
      await waitForTunnelListenerRelease(localPort);
      await waitForFilesystemPath(cleanupLocalPath, false);

      await waitForConnected();
      await displayedElement("aria/Terminal 2").then((button) => button.click());
      await runTerminalMarker("TAB_B_SURVIVED_TAB_A_CLEANUP");
      await displayedElement("aria/Close terminal 1").then((button) => button.click());
      await waitForTerminalCount(1);
      await runTerminalMarker("TAB_B_REMAINING_TERMINAL_WORKS");

      await closeActiveSessionTab();
      await waitForSessionTabCount(0);
      await $("p=Double-click a connection to open a tab.").waitForDisplayed();
    } finally {
      for (const probe of probes) probe.destroy();
    }
  });
});
