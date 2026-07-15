import assert from "node:assert/strict";

import { createFreshVault } from "../../support/app";

async function expectVisibleOutline(
  element: WebdriverIO.Element | ReturnType<typeof $>,
): Promise<void> {
  const shadow = await (element as WebdriverIO.Element).getCSSProperty("box-shadow");
  assert.notEqual(shadow.value, "none", "The pale badge had no contrast outline.");
  assert.notEqual(shadow.value, "", "The pale badge had no contrast outline.");
  assert.doesNotMatch(
    shadow.value?.toString() ?? "",
    /rgba?\(0,\s*0,\s*0(?:,\s*0)?\)/u,
    "The pale badge outline was transparent.",
  );
}

describe("@appearance-theme", () => {
  it("applies and persists a light palette with visible pale badges", async () => {
    await createFreshVault("appearance-theme");

    await $("aria/Settings").click();
    const settings = await $("[role='dialog'][aria-label='Settings']");
    await settings.waitForDisplayed();
    const light = await settings.$("input[name='application-theme'][value='light']");
    await light.click();
    const root = await $("html");
    await browser.waitUntil(async () => (await root.getAttribute("data-theme")) === "light");
    await settings.$("button=Save").click();
    await settings.waitForDisplayed({ reverse: true });

    await $("aria/New folder").click();
    const folderDialog = await $("[role='dialog'][aria-label='New folder']");
    await folderDialog.waitForDisplayed();
    const palePreset = await folderDialog.$("button[aria-label='#e6edf3']");
    await expectVisibleOutline(palePreset);
    await palePreset.click();
    await folderDialog.$("input[placeholder='Clients']").setValue("Pale badge");
    await folderDialog.$("button=Create").click();
    await folderDialog.waitForDisplayed({ reverse: true });

    const folder = await $("[role='treeitem'][aria-label='Pale badge']");
    await folder.waitForDisplayed();
    await expectVisibleOutline(await folder.$(".dot"));

    await $("aria/Settings").click();
    const reopenedSettings = await $("[role='dialog'][aria-label='Settings']");
    await reopenedSettings.waitForDisplayed();
    assert.equal(
      await reopenedSettings.$("input[name='application-theme'][value='light']").isSelected(),
      true,
      "The saved light appearance was not restored.",
    );
  });
});
