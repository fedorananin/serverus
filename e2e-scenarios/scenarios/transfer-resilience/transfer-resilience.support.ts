import { choosePaneAction, openFileOption } from "../../support/files";

export interface ExpectedTransferSummary {
  queued: number;
  running: number;
  done: number;
  failed: number;
  total: number;
}

export function paneFile(side: "local" | "remote", name: string): ReturnType<typeof $> {
  return $(`[data-pane='${side}'] [role='option'][aria-label='${name}']`);
}

export async function enterDirectory(side: "local" | "remote", name: string): Promise<void> {
  const directory = await paneFile(side, name);
  await directory.waitForDisplayed();
  await openFileOption(side, directory);
}

export async function transferByAction(
  side: "local" | "remote",
  name: string,
): Promise<void> {
  const item = await paneFile(side, name);
  await item.waitForDisplayed();
  await choosePaneAction(side, item, side === "local" ? "Upload →" : "← Download");
}

export async function waitForConflict(name?: string): Promise<WebdriverIO.Element> {
  const dialog = await $("[role='dialog'][aria-label='File already exists']");
  await dialog.waitForDisplayed();
  if (name) await dialog.$(`strong=${name}`).waitForDisplayed();
  return dialog as unknown as WebdriverIO.Element;
}

export async function resolveConflict(
  action: "Overwrite" | "Skip" | "Rename",
  applyToAll = false,
): Promise<void> {
  const dialog = await waitForConflict();
  if (applyToAll) {
    const checkbox = await dialog.$("input[type='checkbox']");
    if (!(await checkbox.isSelected())) await checkbox.click();
    await checkbox.waitUntil(() => checkbox.isSelected(), {
      timeoutMsg: "The visible apply-to-all checkbox did not become selected.",
    });
  }
  await dialog.$(`button=${action}`).click();
  await dialog.waitForDisplayed({ reverse: true });
}

export async function waitForTransferState(name: string, state: string): Promise<void> {
  await $(`[data-transfer-name='${name}'][data-state='${state}']`).waitForDisplayed({
    timeout: 60_000,
  });
}

export async function retryTransfer(name: string): Promise<void> {
  const item = await $(`[data-transfer-name='${name}'][data-state='error']`);
  await item.$("button[title='Retry (resumes partial files)']").click();
}

export async function waitForTransferSummary(
  expected: ExpectedTransferSummary,
): Promise<void> {
  const summary = await $("[data-testid='transfer-summary']");
  await summary.waitForDisplayed();
  await summary.waitUntil(
    async () =>
      (await summary.getAttribute("data-queued")) === String(expected.queued) &&
      (await summary.getAttribute("data-running")) === String(expected.running) &&
      (await summary.getAttribute("data-done")) === String(expected.done) &&
      (await summary.getAttribute("data-failed")) === String(expected.failed) &&
      (await summary.getAttribute("data-total")) === String(expected.total),
    {
      timeout: 60_000,
      timeoutMsg: `Transfer summary did not become ${JSON.stringify(expected)}.`,
    },
  );
  const visible = await summary.getText();
  const visibleCounts: Array<[number, string]> = [
    [expected.running, "active"],
    [expected.queued, "queued"],
    [expected.done, "done"],
    [expected.failed, "failed"],
  ];
  for (const [count, label] of visibleCounts) {
    if (count > 0 && !visible.includes(`${count} ${label}`)) {
      throw new Error(`The visible transfer summary did not show ${count} ${label}.`);
    }
    if (count === 0 && visible.includes(label)) {
      throw new Error(`The visible transfer summary unexpectedly showed ${label}.`);
    }
  }
}
