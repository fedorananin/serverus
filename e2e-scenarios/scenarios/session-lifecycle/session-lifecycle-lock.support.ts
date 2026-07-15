import assert from "node:assert/strict";

import { correctVaultPassword } from "../../support/app";

export async function unlockAfterAutoLock(): Promise<void> {
  const overlay = await $("[data-testid='unlock-overlay']");
  await overlay.waitForDisplayed({
    timeout: 30_000,
    timeoutMsg: "Auto-lock did not display the unlock overlay.",
  });

  const main = await $("[data-testid='main-screen']");
  assert.equal(await main.getAttribute("aria-hidden"), "true");
  assert.notEqual(await main.getAttribute("inert"), null);

  const position = await overlay.getCSSProperty("position");
  const background = await overlay.getCSSProperty("background-color");
  assert.equal(position.value, "fixed");
  assert.doesNotMatch(
    String(background.value).replaceAll(" ", ""),
    /^(?:transparent|rgba\(0,0,0,0\))$/iu,
  );

  for (const edge of ["top", "right", "bottom", "left"] as const) {
    assert.equal((await overlay.getCSSProperty(edge)).value, "0px");
  }

  const settings = await main.$("button[aria-label='Settings']");
  assert.equal(await settings.isClickable(), false);

  await $("aria/Master password").setValue(correctVaultPassword(), { mask: true });
  await $("button=Unlock").click();
  await overlay.waitForDisplayed({ reverse: true });
  await main.waitUntil(async () => (await main.getAttribute("aria-hidden")) === "false", {
    timeoutMsg: "Unlocking did not restore the interactive main screen.",
  });

  await $("aria/Settings").click();
  const dialog = await $("[role='dialog'][aria-label='Settings']");
  await dialog.waitForDisplayed();
  const timeout = await dialog.$(
    "input[aria-label='Auto-lock after (minutes, 0 = never)']",
  );
  await timeout.setValue("15");
  await timeout.waitUntil(async () => (await timeout.getValue()) === "15", {
    timeoutMsg: "The visible auto-lock field did not restore its normal timeout.",
  });
  await dialog.$("button=Save").click();
  await dialog.waitForDisplayed({ reverse: true });
}
