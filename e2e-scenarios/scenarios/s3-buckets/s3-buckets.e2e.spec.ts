import { createFreshVault } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import { choosePaneAction, fileOption, openFileOption } from "../../support/files";
import { fixtures, S3_TEST_ACCESS_KEY, S3_TEST_SECRET_KEY } from "../../support/fixtures";

describe("@s3-buckets", () => {
  it("lists and creates buckets through a real S3-compatible server", async () => {
    const fixture = fixtures();
    await createFreshVault("s3-buckets");
    await createConnection({
      name: "Scenario S3",
      protocol: "s3",
      host: fixture.s3.endpoint,
      port: fixture.s3.port,
      username: S3_TEST_ACCESS_KEY,
      password: S3_TEST_SECRET_KEY,
      region: "us-east-1",
      pathStyle: true,
    });
    await openConnection("Scenario S3");
    await waitForConnected();
    await (await fileOption("remote", "serverus-e2e")).waitForDisplayed();

    const bucket = `serverus-scenario-${Date.now()}`;
    const remoteRows = await $("[data-pane='remote'] [role='listbox']");
    await choosePaneAction("remote", remoteRows, "New folder…");
    const dialog = await $("[role='dialog'][aria-label='New folder']");
    await dialog.$("input[placeholder='folder name']").setValue(bucket);
    await dialog.$("button=Create").click();
    const createdBucket = await fileOption("remote", bucket);
    await createdBucket.waitForDisplayed({ timeout: 30_000 });

    await openFileOption("remote", createdBucket);
    const remotePath = await $("[data-pane='remote'] button[aria-label='Scenario S3 path']");
    await remotePath.waitUntil(async () => (await remotePath.getText()) === `/${bucket}`, {
      timeout: 30_000,
      timeoutMsg: "The created S3 bucket did not open as a visible folder.",
    });
    expect(await remotePath.getAttribute("title")).toBe(`/${bucket}`);
    await remoteRows.waitUntil(async () => (await remoteRows.getText()).trim() === "Empty", {
      timeout: 30_000,
      timeoutMsg: "The new S3 bucket did not show an empty listing.",
    });
    expect(await remoteRows.$("[role='option']").isExisting()).toBe(false);
    expect(await $("[data-pane='remote'] .statusbar").getText()).toContain("0 items");
  });
});
