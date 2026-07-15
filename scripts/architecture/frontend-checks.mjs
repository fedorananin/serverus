import { existsSync, readFileSync, readdirSync } from "node:fs";
import { extname, join, relative, sep } from "node:path";

const MODEL_FILES = new Set([
  "src/lib/components/ConflictDialog.svelte",
  "src/lib/components/TransferQueue.svelte",
  "src/lib/stores/transfers.svelte.ts",
  "src/routes/MainScreen.svelte",
]);
const EXTENSIONS = new Set([".js", ".mjs", ".ts", ".tsx", ".svelte"]);

function collect(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return collect(path);
    return entry.isFile() && EXTENSIONS.has(extname(entry.name)) ? [path] : [];
  });
}

function relativePath(root, path) {
  return relative(root, path).split(sep).join("/");
}

function firstMatch(source, pattern) {
  const match = pattern.exec(source);
  pattern.lastIndex = 0;
  return match;
}

function lineOf(source, offset) {
  return source.slice(0, offset).split("\n").length;
}

function isAdapter(path) {
  return (
    path.startsWith("src/lib/api/") ||
    path.startsWith("src/lib/app/adapters/")
  );
}

export function checkFrontend(root) {
  const files = collect(join(root, "src"));
  const errors = [];
  for (const file of files) {
    const path = relativePath(root, file);
    const source = readFileSync(file, "utf8");
    const isTest = /\.(?:test|spec)\.[^.]+$/.test(path);
    if (!isTest) {
      const inlineTest = firstMatch(
        source,
        /(?:from\s*|import\s*\(\s*)["'](?:vitest|@testing-library\/svelte)["']/g,
      );
      if (inlineTest) {
        errors.push(
          `[inline-test-module] ${path}:${lineOf(source, inlineTest.index)} contains frontend test ` +
            `code in a production module. Move it to a separate .test.ts or .spec.ts test file.`,
        );
      }
    }
    if (isAdapter(path)) continue;
    if (!/\btransfers?(?:[A-Z_:-]|-[a-z]|_[a-z])|\bTransfer[A-Z]/.test(source)) continue;

    const direct = firstMatch(source, /\bcommands\s*\.\s*(transfer[A-Z][A-Za-z0-9_]*)\b/g);
    if (direct) {
      errors.push(
        `[frontend-transfer-boundary] ${path}:${lineOf(source, direct.index)} invokes ` +
          `${direct[1]} directly. Route it through the app-scoped TransfersStore.`,
      );
      continue;
    }
    const generated = firstMatch(
      source,
      /(?:from\s*|import\s*\(\s*)["'][^"']*api\/bindings(?:\.ts)?["']/g,
    );
    if (generated) {
      errors.push(
        `[frontend-transfer-boundary] ${path}:${lineOf(source, generated.index)} imports ` +
          `generated bindings directly. Route transfer behavior through AppApi or AppEventSource.`,
      );
      continue;
    }
    const tauri = firstMatch(source, /["']@tauri-apps\/api\/(?:core|event)["']/g);
    if (tauri) {
      errors.push(
        `[frontend-transfer-boundary] ${path}:${lineOf(source, tauri.index)} imports raw Tauri APIs. ` +
          `Move IPC mapping behind AppApi or AppEventSource.`,
      );
      continue;
    }
    if (!MODEL_FILES.has(path)) continue;
    const legacy = firstMatch(
      source,
      /(?:from\s*|import\s*\(\s*)["']\$lib\/api(?:\/index)?["']/g,
    );
    if (legacy) {
      errors.push(
        `[frontend-transfer-boundary] ${path}:${lineOf(source, legacy.index)} bypasses the ` +
          `app-scoped transfer model through the legacy API barrel.`,
      );
    }
  }
  return { errors, checkedFiles: files.length };
}
