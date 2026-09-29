import { expect, it } from "vitest";
import { paneAffected } from "./agent-fs-refresh.svelte";

it("relists the pane showing the changed path or its parent", () => {
  expect(paneAffected("/var/www", "/var/www/new-dir")).toBe(true);
  expect(paneAffected("/var/www/", "/var/www/new-dir/")).toBe(true);
  expect(paneAffected("/var/www", "/var/www")).toBe(true);
  expect(paneAffected("/", "/etc")).toBe(true);
  expect(paneAffected("/var", "/var/www/deep/file")).toBe(false);
  expect(paneAffected("/srv", "/var/www")).toBe(false);
});
