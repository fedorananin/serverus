export function fileOption(
  side: "local" | "remote",
  name: string,
): ReturnType<typeof $> {
  return $(`[data-pane='${side}'] [role='option'][aria-label='${name}']`);
}

export async function openFileOption(
  side: "local" | "remote",
  option: WebdriverIO.Element | ReturnType<typeof $>,
): Promise<void> {
  await choosePaneAction(side, option, "Open");
}

export async function choosePaneAction(
  side: "local" | "remote",
  target: WebdriverIO.Element | ReturnType<typeof $>,
  action: string,
): Promise<void> {
  const element = target as WebdriverIO.Element;
  await element.click();
  await chooseSelectedPaneAction(side, action);
}

export async function chooseSelectedPaneAction(
  side: "local" | "remote",
  action: string,
): Promise<void> {
  await $(`[data-pane='${side}'] [aria-label='${side === "local" ? "Local" : "Remote"} pane actions']`).click();
  const item = await $(`aria/${action}`);
  await item.waitForDisplayed({
    timeout: 5_000,
    timeoutMsg: `The visible pane actions did not expose ${action}.`,
  });
  await item.click();
}

export async function refreshPane(side: "local" | "remote"): Promise<void> {
  await $(`[data-pane='${side}'] [aria-label='Refresh']`).click();
}

export async function waitForCompletedTransfers(expectedDone: number): Promise<void> {
  const summary = await $("[data-testid='transfer-summary']");
  await summary.waitUntil(
    async () => {
      const done = Number(await summary.getAttribute("data-done"));
      const total = Number(await summary.getAttribute("data-total"));
      return (
        done === expectedDone &&
        total === expectedDone &&
        (await summary.getAttribute("data-failed")) === "0" &&
        (await summary.getAttribute("data-running")) === "0" &&
        (await summary.getAttribute("data-queued")) === "0"
      );
    },
    {
      timeout: 120_000,
      timeoutMsg: `Expected exactly ${expectedDone} completed transfer(s) and an idle queue.`,
    },
  );
  const visible = await summary.getText();
  if (!visible.includes("Transfers") || !visible.includes(`${expectedDone} done`)) {
    throw new Error(`The visible transfer summary did not show ${expectedDone} done.`);
  }
  for (const unexpected of ["active", "queued", "failed"]) {
    if (visible.includes(unexpected)) {
      throw new Error(`The idle transfer summary still showed ${unexpected}.`);
    }
  }
}
