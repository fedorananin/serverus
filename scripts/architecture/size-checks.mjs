import { existsSync, readFileSync, readdirSync } from "node:fs";
import { extname, join, relative, sep } from "node:path";

export const SOURCE_LINE_LIMIT = 300;

// Frozen legacy debt: these files predate the architecture migration. Their
// caps prevent further growth; touching one materially requires splitting it
// and deleting its entry instead of raising the cap.
const LEGACY_OVERSIZED_FILES = new Map([
  // Protocol adapters remain intact until the capability-based endpoint
  // migration, so the current phase does not mix a move with a redesign.
  ["src-tauri/src/session/s3.rs", 1058],
  ["src-tauri/src/session/ssh.rs", 359],
  // Platform unlock split remains in migration phase 3.
  ["src-tauri/src/vault/quick_unlock.rs", 431],
]);

const EXTENSIONS = new Set([".rs", ".js", ".mjs", ".ts", ".tsx", ".svelte"]);
const GENERATED_FILES = new Set(["src/lib/api/bindings.ts"]);

function relativePath(root, path) {
  return relative(root, path).split(sep).join("/");
}

function collect(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return collect(path);
    return entry.isFile() && EXTENSIONS.has(extname(entry.name)) ? [path] : [];
  });
}

function physicalLineCount(source) {
  if (source.length === 0) return 0;
  return source.split(/\r?\n/).length - (source.endsWith("\n") ? 1 : 0);
}

export function checkSourceFileSizes(root) {
  const files = [
    ...collect(join(root, "src-tauri", "src")),
    ...collect(join(root, "src-tauri", "tests")),
    ...collect(join(root, "crates")),
    ...collect(join(root, "src")),
    ...collect(join(root, "scripts")),
  ];
  const errors = [];
  for (const file of files) {
    const path = relativePath(root, file);
    if (GENERATED_FILES.has(path)) continue;
    const lines = physicalLineCount(readFileSync(file, "utf8"));
    if (lines <= SOURCE_LINE_LIMIT) continue;
    const frozenCap = LEGACY_OVERSIZED_FILES.get(path);
    if (frozenCap !== undefined && lines <= frozenCap) continue;
    errors.push(
      `[source-file-size] ${path} has ${lines} physical lines; the normal limit is ` +
        `${SOURCE_LINE_LIMIT}. Split the file by responsibility.`,
    );
  }
  return { errors, checkedFiles: files.length };
}
