async function displayedElement(selector: string): Promise<WebdriverIO.Element> {
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
    { timeout: 30_000, timeoutMsg: `No displayed terminal control matched ${selector}.` },
  );
  if (!displayed) throw new Error(`No displayed terminal control matched ${selector}.`);
  return displayed;
}

export async function expectTerminalText(text: string): Promise<void> {
  await (await displayedElement("aria/Open terminal find")).click();
  const bar = await displayedElement(".find-bar");
  await bar.$("aria/Terminal find text").setValue(text);
  const result = await bar.$("[role='status']");
  await result.waitUntil(async () => (await result.getText()) === "Match found", {
    timeout: 30_000,
    timeoutMsg: `The visible terminal search did not find ${text}.`,
  });
  await bar.$("button[title='Close']").click();
  await bar.waitForDisplayed({ reverse: true });
}

export async function pasteTerminalText(text: string): Promise<void> {
  await (await displayedElement("aria/Open terminal paste dialog")).click();
  const dialog = await displayedElement("[role='dialog'][aria-label='Paste into terminal']");
  const input = await dialog.$("aria/Terminal paste text");
  await input.setValue(text);
  await input.waitUntil(async () => (await input.getValue()) === text, {
    timeoutMsg: "The visible terminal paste field did not retain the entered text.",
  });
  await dialog.$("button=Paste and run").click();
  await dialog.waitForDisplayed({ reverse: true });
}
