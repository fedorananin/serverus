import { existsSync } from "node:fs";
import { readdir } from "node:fs/promises";
import { createServer, createConnection as connectTcp, type Socket } from "node:net";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { Key } from "webdriverio";

import { fixtures } from "../../support/fixtures";
import {
  expectTerminalText,
  pasteTerminalText,
} from "../../support/terminal";

const EDIT_CACHE = join(tmpdir(), "serverus-edit");

export async function displayedElement(selector: string): Promise<WebdriverIO.Element> {
  let displayed: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const elements = await $$(selector).getElements();
      for (const element of elements) {
        if (await element.isDisplayed()) {
          displayed = element;
          return true;
        }
      }
      return false;
    },
    { timeout: 30_000, timeoutMsg: `No displayed element matched ${selector}.` },
  );
  if (!displayed) throw new Error(`No displayed element matched ${selector}.`);
  return displayed;
}

export async function reserveLocalPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Could not reserve a local port.");
  await new Promise<void>((resolve) => server.close(() => resolve()));
  return address.port;
}

export async function configureScenarioEditor(): Promise<void> {
  await $("aria/Settings").click();
  const dialog = await $("[role='dialog'][aria-label='Settings']");
  await dialog.waitForDisplayed();
  const useDefault = await dialog.$(
    "//label[contains(., 'Open remote files with the system default app')]/input[@type='checkbox']",
  );
  if (await useDefault.isSelected()) await useDefault.click();
  const application = await dialog.$(
    "//label[span[normalize-space()='Application']]/input[@type='text']",
  );
  await application.waitForDisplayed();
  await application.setValue("/usr/bin/true");
  await application.waitUntil(async () => (await application.getValue()) === "/usr/bin/true", {
    timeoutMsg: "The visible editor field did not retain the scenario editor path.",
  });
  await dialog.$("button=Save").click();
  await dialog.waitForDisplayed({ reverse: true });
}

export async function configureOneMinuteAutoLock(): Promise<void> {
  await $("aria/Settings").click();
  const dialog = await $("[role='dialog'][aria-label='Settings']");
  await dialog.waitForDisplayed();
  const timeout = await dialog.$(
    "input[aria-label='Auto-lock after (minutes, 0 = never)']",
  );
  await timeout.setValue("1");
  await timeout.waitUntil(async () => (await timeout.getValue()) === "1", {
    timeoutMsg: "The visible auto-lock field did not retain the one-minute setting.",
  });
  await dialog.$("button=Save").click();
  await dialog.waitForDisplayed({ reverse: true });
}

export async function createScenarioSshConnection(localPort: number): Promise<void> {
  const ssh = fixtures().ssh;
  if (!ssh.available) throw new Error("SSH fixture is unavailable.");

  await $("button=+ Connection").click();
  const dialog = await $("[role='dialog'][aria-label='New connection']");
  await dialog.waitForDisplayed();
  await dialog.$("aria/Connection name").setValue("Lifecycle SSH");
  await dialog.$("aria/Connection host").setValue(ssh.host);
  await dialog.$("aria/Connection port").setValue(String(ssh.port));
  await dialog.$("aria/Connection username").setValue(ssh.username);

  const auth = await dialog.$("input[type='radio'][value='key']");
  await auth.click();
  await auth.waitUntil(async () => auth.isSelected(), {
    timeoutMsg: "The visible key-authentication input was not selected.",
  });
  await dialog.$("aria/SSH private key path").setValue(ssh.key_path);
  const remoteStart = await dialog.$("input[aria-label='Remote start directory']");
  const remoteStartPath = join(fixtures().paths.ssh_root, "serverus-e2e/site");
  await remoteStart.setValue(remoteStartPath);
  await remoteStart.waitUntil(async () => (await remoteStart.getValue()) === remoteStartPath, {
    timeoutMsg: "The visible remote start directory field did not retain its value.",
  });
  await dialog.$("aria/Local start directory").setValue(fixtures().paths.local_download);

  await dialog.$("button=+ Add tunnel").click();
  await dialog.$("input[placeholder='name']").setValue("Scenario tunnel");
  await dialog.$("input[title='Local port']").setValue(String(localPort));
  await dialog.$("input[placeholder='127.0.0.1']").setValue("127.0.0.1");
  await dialog.$("input[title='Remote port']").setValue(String(fixtures().s3.port));
  await dialog.$("button=Create").click();
  await dialog.waitForDisplayed({ reverse: true });
  await $("aria/Lifecycle SSH").waitForDisplayed();
}

export async function openScenarioConnection(): Promise<void> {
  const connection = await $("aria/Lifecycle SSH");
  await connection.click();
  await browser.keys(Key.Enter);
}

export async function acceptUnknownHost(): Promise<void> {
  const dialog = await $("[role='dialog'][aria-label='Unknown host']");
  await dialog.waitForDisplayed({ timeout: 30_000 });
  await dialog.$("button=Trust and connect").click();
}

export async function waitForConnected(): Promise<void> {
  await displayedElement("[data-testid='session-state'][data-state='connected']");
}

export async function waitForSessionTabCount(count: number): Promise<void> {
  await browser.waitUntil(async () => (await $$("[role='tab']").getElements()).length === count, {
    timeoutMsg: `Expected ${count} session tab(s).`,
  });
}

export async function activateSessionTab(index: number): Promise<void> {
  const tabs = await $$("[role='tab']").getElements();
  const tab = tabs[index];
  if (!tab) throw new Error(`Session tab ${index + 1} does not exist.`);
  await tab.click();
  await tab.waitUntil(async () => (await tab.getAttribute("aria-selected")) === "true");
}

export async function closeActiveSessionTab(): Promise<void> {
  const tabs = await $$("[role='tab']").getElements();
  for (const tab of tabs) {
    if ((await tab.getAttribute("aria-selected")) === "true") {
      await tab.$("aria/Close tab").click();
      return;
    }
  }
  throw new Error("No active session tab exists.");
}

export async function runTerminalMarker(marker: string): Promise<void> {
  if (!/^[A-Z0-9_]+$/u.test(marker) || marker.length < 2) {
    throw new Error("Terminal markers must be safe, non-trivial shell tokens.");
  }
  await displayedElement("[data-terminal-state='ready'] .xterm-screen");
  await pasteTerminalText(`printf '%s_%s\\n' '${marker}' "$((6 * 7))"\n`);
  await expectTerminalText(`${marker}_42`);
}

export async function editCacheDirectories(): Promise<Set<string>> {
  try {
    const entries = await readdir(EDIT_CACHE, { withFileTypes: true });
    return new Set(entries.filter((entry) => entry.isDirectory()).map((entry) => entry.name));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return new Set();
    throw error;
  }
}

export async function waitForNewEditCache(baseline: Set<string>): Promise<Set<string>> {
  let created = new Set<string>();
  await browser.waitUntil(
    async () => {
      const current = await editCacheDirectories();
      created = new Set([...current].filter((name) => !baseline.has(name)));
      return created.size > 0;
    },
    { timeout: 30_000, timeoutMsg: "Remote edit did not create a watched cache file." },
  );
  return created;
}

export async function waitForEditCacheCleanup(created: Set<string>): Promise<void> {
  await browser.waitUntil(
    async () => {
      const current = await editCacheDirectories();
      return [...created].every((name) => !current.has(name));
    },
    { timeout: 30_000, timeoutMsg: "Closing the tab did not remove its remote-edit cache." },
  );
}

export async function waitForFilesystemPath(path: string, exists: boolean): Promise<void> {
  await browser.waitUntil(() => existsSync(path) === exists, {
    timeout: 30_000,
    timeoutMsg: exists
      ? `Transfer did not create its partial target at ${path}.`
      : `Session cleanup left its partial target at ${path}.`,
  });
}

async function tryTunnelConnection(port: number): Promise<Socket | null> {
  return new Promise((resolve) => {
    const socket = connectTcp({ host: "127.0.0.1", port });
    const finish = (connected: boolean) => {
      socket.off("connect", onConnect);
      socket.off("error", onError);
      if (connected) resolve(socket);
      else {
        socket.destroy();
        resolve(null);
      }
    };
    const onConnect = () => finish(true);
    const onError = () => finish(false);
    socket.once("connect", onConnect);
    socket.once("error", onError);
  });
}

export async function openTunnelProbe(port: number, purpose: string): Promise<Socket> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const socket = await tryTunnelConnection(port);
    if (socket) return socket;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`The tunnel did not accept its ${purpose} probe.`);
}

export async function sendTunnelRequest(socket: Socket): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const finish = (error?: Error) => {
      clearTimeout(timeout);
      socket.off("data", onData);
      socket.off("error", onError);
      socket.off("close", onClose);
      if (error) reject(error);
      else resolve();
    };
    const onData = (data: Buffer) =>
      finish(
        String(data).startsWith("HTTP/1.1")
          ? undefined
          : new Error("Tunnel returned invalid HTTP traffic."),
      );
    const onError = (error: Error) => finish(error);
    const onClose = () => finish(new Error("Tunnel closed before returning HTTP traffic."));
    const timeout = setTimeout(
      () => finish(new Error("Tunnel did not return HTTP traffic in time.")),
      15_000,
    );
    socket.once("data", onData);
    socket.once("error", onError);
    socket.once("close", onClose);
    socket.write("GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\r\n");
  });
}

export async function waitForSocketClose(socket: Socket): Promise<void> {
  if (socket.closed || socket.destroyed) return;
  await new Promise<void>((resolve, reject) => {
    const onClose = () => {
      clearTimeout(timeout);
      resolve();
    };
    const timeout = setTimeout(() => {
      socket.off("close", onClose);
      reject(new Error("Closing the tunnel left a connection alive."));
    }, 15_000);
    socket.once("close", onClose);
  });
}

export async function waitForTunnelListenerRelease(port: number): Promise<void> {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const socket = await tryTunnelConnection(port);
    if (!socket) return;
    socket.destroy();
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Closing the tab did not release its tunnel listener.");
}
