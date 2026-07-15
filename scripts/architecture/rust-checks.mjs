import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, extname, join, relative, sep } from "node:path";

const CORE_SOURCE_RULES = [
  {
    packageName: "serverus-domain",
    code: "rust-domain-source",
    guidance: "The domain crate is synchronous and effect-free; move I/O and task ownership outward.",
    forbidden: [
      ["std::fs", /\bstd\s*::\s*fs\b/g],
      ["std::net", /\bstd\s*::\s*net\b/g],
      ["std::process", /\bstd\s*::\s*process\b/g],
      ["std::thread", /\bstd\s*::\s*thread\b/g],
      ["std::sync::Mutex", /\bstd\s*::\s*sync\s*::(?:\s*\{[^}]*\b)?Mutex\b/g],
      ["std::sync::RwLock", /\bstd\s*::\s*sync\s*::(?:\s*\{[^}]*\b)?RwLock\b/g],
      ["std::sync::mpsc", /\bstd\s*::\s*sync\s*::\s*mpsc\b/g],
      ["SystemTime", /\b(?:std\s*::\s*time\s*::\s*)?SystemTime\b/g],
      ["Instant", /\b(?:std\s*::\s*time\s*::\s*)?Instant\b/g],
    ],
  },
  {
    packageName: "serverus-application",
    code: "rust-application-source",
    guidance: "Represent files, clocks, processes, and task spawning as application ports.",
    forbidden: [
      ["std::fs", /\bstd\s*::\s*fs\b/g],
      ["std::net", /\bstd\s*::\s*net\b/g],
      ["std::process", /\bstd\s*::\s*process\b/g],
      ["thread::sleep", /\b(?:std\s*::\s*)?thread\s*::\s*sleep\b/g],
      ["SystemTime", /\b(?:std\s*::\s*time\s*::\s*)?SystemTime\b/g],
      ["tokio::spawn", /\btokio\s*::\s*spawn\b/g],
      ["tokio::time::sleep", /\btokio\s*::\s*time\s*::\s*sleep\b/g],
    ],
  },
];

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

function collectRustFiles(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return collectRustFiles(path);
    return entry.isFile() && extname(entry.name) === ".rs" ? [path] : [];
  });
}

function isDedicatedRustTestSource(path) {
  return path.includes("/tests/") || path.endsWith("/tests.rs") || path.endsWith("_tests.rs");
}

export function checkCoreSourcePolicy(root, packages) {
  const errors = [];
  for (const rule of CORE_SOURCE_RULES) {
    const pkg = packages.find(({ name }) => name === rule.packageName);
    if (!pkg) continue;
    for (const file of collectRustFiles(join(dirname(pkg.manifest_path), "src"))) {
      const source = readFileSync(file, "utf8");
      for (const [token, pattern] of rule.forbidden) {
        const match = firstMatch(source, pattern);
        if (!match) continue;
        errors.push(
          `[${rule.code}] ${relativePath(root, file)}:${lineOf(source, match.index)} uses ` +
            `forbidden core dependency "${token}". ${rule.guidance}`,
        );
      }
    }
  }
  return errors;
}

export function checkRustTestPolicy(root, packages) {
  const errors = [];
  for (const pkg of packages) {
    for (const file of collectRustFiles(dirname(pkg.manifest_path))) {
      const path = relativePath(root, file);
      const source = readFileSync(file, "utf8");
      const ignored = firstMatch(source, /#\s*\[\s*ignore(?:\s*=\s*[^\]]+)?\s*\]/g);
      if (ignored) {
        errors.push(
          `[rust-test-policy] ${path}:${lineOf(source, ignored.index)} contains an ignored Rust ` +
            `test. Fix or redesign it; CI must not hide flaky coverage.`,
        );
      }
      if (!path.includes("/src/")) continue;
      if (!isDedicatedRustTestSource(path)) {
        const directTest = firstMatch(
          source,
          /#\s*\[\s*(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)?test(?:\s*\([^\]]*\))?\s*\]/g,
        );
        if (directTest) {
          errors.push(
            `[inline-test-body] ${path}:${lineOf(source, directTest.index)} contains a test ` +
              `function in production source. Move it to a dedicated test module or test target.`,
          );
        }
      }
      const inline = firstMatch(
        source,
        /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+[A-Za-z0-9_]+\s*\{/g,
      );
      if (inline) {
        errors.push(
          `[inline-test-module] ${path}:${lineOf(source, inline.index)} contains test bodies beside ` +
            `production code. Move them to a sibling test module or top-level integration test.`,
        );
      }
    }
  }
  return errors;
}

export function checkDesktopState(root) {
  const path = join(root, "src-tauri", "src", "state.rs");
  if (!existsSync(path)) return [];
  const source = readFileSync(path, "utf8");
  const declaration = /\bpub\s+struct\s+AppState\s*\{([^}]*)\}/m.exec(source);
  if (!declaration) {
    return [`[desktop-state-boundary] ${relativePath(root, path)} must declare one AppState handle.`];
  }

  const errors = [];
  for (const field of ["activity", "edits", "quick", "sessions", "transfers", "vault"]) {
    if (new RegExp(`\\b${field}\\s*:`).test(declaration[1])) {
      errors.push(
        `[desktop-state-boundary] ${relativePath(root, path)} exposes manager field "${field}" ` +
          `directly. Keep it behind AppState.application.`,
      );
    }
  }
  if (!/\bapplication\s*:/.test(declaration[1])) {
    errors.push(`[desktop-state-boundary] ${relativePath(root, path)} must expose AppState.application.`);
  }
  return errors;
}
