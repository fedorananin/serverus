import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, test } from "node:test";

import { verifyBindings } from "./check-bindings.mjs";

const roots = [];

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true })));
});

async function fixture(contents = "current\n") {
  const root = await mkdtemp(join(tmpdir(), "serverus-bindings-"));
  roots.push(root);
  const path = join(root, "src/lib/api/bindings.ts");
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents);
  return { root, path };
}

test("accepts deterministic generation", async () => {
  const { root, path } = await fixture();

  const result = verifyBindings(root, () => ({ status: 0 }));

  assert.equal(result.current, true);
  assert.equal(await readFile(path, "utf8"), "current\n");
});

test("accepts generated bindings with platform-native line endings", async () => {
  const { root, path } = await fixture("export const current = true;\r\n");

  const result = verifyBindings(root, () => {
    writeFileSync(path, "export const current = true;\n");
    return { status: 0 };
  });

  assert.equal(result.current, true);
  assert.equal(await readFile(path, "utf8"), "export const current = true;\r\n");
});

test("reports stale bindings and restores the original working file", async () => {
  const { root, path } = await fixture();

  const result = verifyBindings(root, () => {
    // The real generator writes the committed path; verification must still
    // be observational and leave a developer's working tree untouched.
    writeFileSync(path, "generated\n");
    return { status: 0 };
  });

  assert.equal(result.current, false);
  assert.equal(await readFile(path, "utf8"), "current\n");
});
