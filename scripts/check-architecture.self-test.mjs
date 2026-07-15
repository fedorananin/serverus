import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { afterEach, test } from "node:test";

// Real temporary workspaces keep the checker tests independent of the repository state.

const checkerPath = resolve(import.meta.dirname, "check-architecture.mjs");
const fixtureRoots = [];

afterEach(async () => {
  await Promise.all(fixtureRoots.splice(0).map((root) => rm(root, { recursive: true })));
});

async function put(root, path, contents) {
  const target = join(root, path);
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, contents);
}

async function addPackage(root, path, name, dependencies = "") {
  await put(
    root,
    `${path}/Cargo.toml`,
    `[package]\nname = "${name}"\nversion = "0.0.0"\nedition = "2021"\n${dependencies}`,
  );
  await put(root, `${path}/src/lib.rs`, "pub fn fixture() {}\n");
}

async function createFixture({ violations = false } = {}) {
  const root = await mkdtemp(join(tmpdir(), "serverus-architecture-"));
  fixtureRoots.push(root);

  const members = [
    "crates/serverus-domain",
    "crates/serverus-application",
    "crates/serverus-runtime",
    "crates/serverus-adapters",
    ...(violations ? [] : ["crates/serverus-testkit"]),
    "src-tauri",
  ];
  await put(
    root,
    "Cargo.toml",
    `[workspace]\nmembers = ${JSON.stringify(members)}\n${violations ? 'exclude = ["crates/serverus-testkit"]\n' : ""}resolver = "2"\n`,
  );

  await addPackage(
    root,
    "crates/serverus-domain",
    "serverus-domain",
    violations ? `[dependencies]\ntauri = { path = "../../vendor/tauri" }\n` : "",
  );
  await addPackage(
    root,
    "crates/serverus-application",
    "serverus-application",
    violations
      ? `[dependencies]\nserverus-adapters = { path = "../serverus-adapters" }\nrussh = { path = "../../vendor/russh" }\n`
      : `[dependencies]\nserverus-domain = { path = "../serverus-domain" }\n`,
  );
  await addPackage(
    root,
    "crates/serverus-runtime",
    "serverus-runtime",
    violations ? `[dependencies]\ntauri = { path = "../../vendor/tauri" }\n` : "",
  );
  await addPackage(
    root,
    "crates/serverus-adapters",
    "serverus-adapters",
    violations
      ? `[dependencies]\nserverus-runtime = { path = "../serverus-runtime" }\n`
      : "",
  );
  await addPackage(root, "crates/serverus-testkit", "serverus-testkit");
  await addPackage(
    root,
    "src-tauri",
    "serverus",
    violations
      ? `[dependencies]\nserverus-testkit = { path = "../crates/serverus-testkit" }\n`
      : "",
  );
  await addPackage(root, "vendor/tauri", "tauri");
  await addPackage(root, "vendor/russh", "russh");

  if (violations) {
    await put(
      root,
      "crates/serverus-domain/src/lib.rs",
      `use std::fs;\nuse std::sync::Mutex;\n\npub fn hidden_io() {\n  let _ = fs::read("vault");\n  let _ = Mutex::new(());\n}\n\n#[cfg(test)]\nmod tests {\n  #[test]\n  #[ignore = "flaky"]\n  fn hidden_failure() {}\n}\n`,
    );
    await put(
      root,
      "crates/serverus-application/src/lib.rs",
      `pub async fn hidden_runtime() {\n  std::thread::sleep(std::time::Duration::from_secs(1));\n}\n`,
    );
    await put(
      root,
      "src-tauri/src/state.rs",
      `pub struct AppState {\n  pub application: (),\n  pub vault: (),\n}\n`,
    );
    await put(
      root,
      "src/lib/components/TransferQueue.svelte",
      `<script>\n  import { commands } from "$lib/api/bindings";\n  void commands.transferPause("transfer-1");\n</script>\n`,
    );
    await put(
      root,
      "src/lib/stores/transfers.svelte.ts",
      `import { listen } from "@tauri-apps/api/event";\nvoid listen("transfer-progress", () => {});\n`,
    );
  } else {
    await put(
      root,
      "src-tauri/src/state.rs",
      `pub struct AppState {\n  pub application: (),\n}\n`,
    );
    await put(
      root,
      "src/lib/app/adapters/tauri-transfers.ts",
      `import { commands } from "$lib/api/bindings";\nexport const list = () => commands.transferList();\n`,
    );
    await put(
      root,
      "src/lib/components/TransferQueue.svelte",
      `<script>\n  import { useAppModel } from "$lib/app/model.svelte";\n  const transfers = useAppModel().transfers;\n</script>\n`,
    );
  }

  return root;
}

function runChecker(root) {
  return spawnSync(process.execPath, [checkerPath, "--root", root], {
    encoding: "utf8",
  });
}

test("reports actionable boundary violations from an isolated fixture", async () => {
  const result = runChecker(await createFixture({ violations: true }));

  assert.equal(result.status, 1);
  assert.match(result.stderr, /\[workspace-member\].*serverus-testkit/s);
  assert.match(result.stderr, /\[rust-domain-dependency\].*tauri/s);
  assert.match(result.stderr, /\[rust-application-dependency\].*serverus-adapters/s);
  assert.match(result.stderr, /\[rust-application-dependency\].*russh/s);
  assert.match(result.stderr, /\[rust-runtime-dependency\].*tauri/s);
  assert.match(
    result.stderr,
    /\[rust-internal-dependency\].*serverus-adapters.*serverus-runtime/s,
  );
  assert.match(
    result.stderr,
    /\[rust-internal-dependency\].*serverus.*serverus-testkit.*normal/s,
  );
  assert.match(result.stderr, /\[rust-domain-source\].*std::fs/s);
  assert.match(result.stderr, /\[rust-domain-source\].*Mutex/s);
  assert.match(result.stderr, /\[rust-application-source\].*thread::sleep/s);
  assert.match(result.stderr, /\[rust-test-policy\].*serverus-domain\/src\/lib\.rs/s);
  assert.match(result.stderr, /\[desktop-state-boundary\].*vault/s);
  assert.match(
    result.stderr,
    /\[frontend-transfer-boundary\].*src\/lib\/components\/TransferQueue\.svelte/s,
  );
  assert.match(
    result.stderr,
    /\[frontend-transfer-boundary\].*src\/lib\/stores\/transfers\.svelte\.ts/s,
  );
});

test("accepts inward Rust dependencies and frontend adapter imports", async () => {
  const result = runChecker(await createFixture());

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Architecture boundaries OK/);
});

test("rejects direct transfer commands outside the frontend adapter and store", async () => {
  const root = await createFixture();
  await put(
    root,
    "src/lib/components/FilesView.svelte",
    `<script>\n  import { commands } from "$lib/api";\n  void commands.transferUpload("session", "/local/file", "/remote");\n  void commands.transferDownload("session", "/remote/file", "/local");\n</script>\n`,
  );

  const result = runChecker(root);

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /\[frontend-transfer-boundary\].*src\/lib\/components\/FilesView\.svelte.*transferUpload/s,
  );
});

test("does not treat frontend contract modules as Tauri adapters", async () => {
  const root = await createFixture();
  await put(
    root,
    "src/lib/app/api.ts",
    `import { commands } from "$lib/api";\nexport interface AppApi {}\nvoid commands.transferList();\n`,
  );

  const result = runChecker(root);

  assert.equal(result.status, 1);
  assert.match(result.stderr, /\[frontend-transfer-boundary\].*src\/lib\/app\/api\.ts/s);
});

test("rejects handwritten source files above 300 lines", async () => {
  const root = await createFixture();
  await put(root, "src-tauri/src/oversized.rs", "// line\n".repeat(301));

  const result = runChecker(root);

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /\[source-file-size\].*src-tauri\/src\/oversized\.rs.*301.*limit is 300/s,
  );
});

test("does not exempt oversized frontend components", async () => {
  const root = await createFixture();
  const components = ["FilePane.svelte", "ConnectionDialog.svelte", "SettingsDialog.svelte", "Sidebar.svelte"];
  await Promise.all(components.map((name) =>
    put(root, `src/lib/components/${name}`, "<!-- line -->\n".repeat(301))));
  const result = runChecker(root);
  assert.equal(result.status, 1);
  for (const name of components) {
    assert.match(
      result.stderr,
      new RegExp(String.raw`\[source-file-size\].*${name.replace(".", String.raw`\.`)}.*301`, "s"),
    );
  }
});

test("rejects desktop integration tests above 300 lines", async () => {
  const root = await createFixture();
  await put(root, "src-tauri/tests/oversized.rs", "// line\n".repeat(301));

  const result = runChecker(root);

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /\[source-file-size\].*src-tauri\/tests\/oversized\.rs.*301.*limit is 300/s,
  );
});

test("rejects inline Rust test bodies in production modules", async () => {
  const root = await createFixture();
  await put(
    root,
    "src-tauri/src/inline_tests.rs",
    `pub fn production() {}\n\n#[cfg(test)]\nmod tests {\n  #[test]\n  fn adjacent() {}\n}\n`,
  );

  const result = runChecker(root);

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /\[inline-test-module\].*src-tauri\/src\/inline_tests\.rs.*sibling test module/s,
  );
});

test("rejects direct Rust test functions in production modules", async () => {
  const root = await createFixture();
  for (const [name, attribute] of [["unit", "#[test]"], ["async", "#[tokio::test]"]]) {
    await put(root, `src-tauri/src/${name}_worker.rs`, `${attribute}\nfn hidden_test() {}\n`);
  }

  const result = runChecker(root);

  assert.equal(result.status, 1);
  assert.match(result.stderr, /\[inline-test-body\].*unit_worker\.rs/s);
  assert.match(result.stderr, /\[inline-test-body\].*async_worker\.rs/s);
});

test("rejects frontend test bodies in production modules", async () => {
  const root = await createFixture();
  await put(
    root,
    "src/lib/worker.ts",
    `import { describe, it } from "vitest";\n\nexport function work() {}\n\ndescribe("work", () => {\n  it("runs", work);\n});\n`,
  );

  const result = runChecker(root);

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /\[inline-test-module\].*src\/lib\/worker\.ts.*separate.*test file/s,
  );
});
