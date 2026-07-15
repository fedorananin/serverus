import { Key } from "webdriverio";

interface ScenarioConnection {
  name: string;
  protocol: "ssh" | "ftp" | "s3";
  host: string;
  port: number;
  username: string;
  password?: string;
  authMethod?: "password" | "key" | "agent";
  keyPath?: string;
  localDir?: string;
  region?: string;
  pathStyle?: boolean;
  uploadAcl?: "private" | "public_read" | "ask";
  publicBaseUrl?: string;
}

async function field(dialog: ReturnType<typeof $>, label: string): Promise<WebdriverIO.Element> {
  const element = dialog.$(`aria/${label}`);
  await element.waitForExist();
  return element as unknown as WebdriverIO.Element;
}

async function selectRadio(
  dialog: ReturnType<typeof $>,
  group: string,
  value: string,
): Promise<void> {
  const input = await dialog.$(`input[name='${group}'][value='${value}']`);
  await input.waitForDisplayed();
  await input.click();
  await input.waitUntil(() => input.isSelected(), {
    timeoutMsg: `${group} did not change to ${value}.`,
  });
}

export async function createConnection(connection: ScenarioConnection): Promise<void> {
  await $("button=+ Connection").click();
  const dialog = await $("[role='dialog'][aria-label='New connection']");
  await dialog.waitForDisplayed();

  await (await field(dialog, "Connection name")).setValue(connection.name);
  await selectRadio(dialog, "connection-protocol", connection.protocol);
  await (await field(dialog, "Connection host")).setValue(connection.host);
  await (await field(dialog, "Connection port")).setValue(String(connection.port));
  await (await field(dialog, "Connection username")).setValue(connection.username, {
    mask: true,
  });

  if (connection.authMethod && connection.protocol === "ssh") {
    const method = await dialog.$(
      `input[name='ssh-auth-method'][value='${connection.authMethod}']`,
    );
    await method.click();
    await method.waitUntil(() => method.isSelected(), {
      timeoutMsg: `SSH authentication method did not change to ${connection.authMethod}.`,
    });
  }
  if (connection.keyPath) {
    await (await field(dialog, "SSH private key path")).setValue(connection.keyPath);
  }
  if (connection.localDir) {
    await (await field(dialog, "Local start directory")).setValue(connection.localDir);
  }
  if (connection.region) {
    await (await field(dialog, "S3 region")).setValue(connection.region);
  }
  if (connection.uploadAcl) {
    await selectRadio(dialog, "s3-upload-access", connection.uploadAcl);
  }
  if (connection.publicBaseUrl) {
    await (await field(dialog, "S3 public base URL")).setValue(connection.publicBaseUrl);
  }
  if (connection.pathStyle) {
    const checkbox = await field(dialog, "S3 path-style URLs");
    if (!(await checkbox.isSelected())) await checkbox.click();
  }
  if (connection.password !== undefined) {
    await dialog.$("button=hide").click();
    const password = await field(dialog, "Connection password");
    await password.waitUntil(async () => (await password.getAttribute("type")) === "password", {
      timeoutMsg: "Connection password field was not masked.",
    });
    await password.setValue(connection.password, { mask: true });
  }
  await dialog.$("button=Create").click();
  await dialog.waitForDisplayed({ reverse: true });
  await $(`aria/${connection.name}`).waitForDisplayed();
}

export async function openConnection(name: string): Promise<void> {
  const item = await $(`aria/${name}`);
  await item.waitForDisplayed();
  await item.click();
  await browser.keys(Key.Enter);
}

export async function waitForConnected(): Promise<void> {
  await $("[data-testid='session-state'][data-state='connected']").waitForDisplayed({
    timeout: 30_000,
  });
}
