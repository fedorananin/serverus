import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";

const REQUIRED_WORKSPACE_MEMBERS = [
  { packageName: "serverus-domain", path: "crates/serverus-domain" },
  { packageName: "serverus-application", path: "crates/serverus-application" },
  { packageName: "serverus-runtime", path: "crates/serverus-runtime" },
  { packageName: "serverus-adapters", path: "crates/serverus-adapters" },
  { packageName: "serverus-testkit", path: "crates/serverus-testkit" },
  { packageName: "serverus", path: "src-tauri" },
];

const PROTOCOL_AND_IPC_DEPENDENCIES = new Set([
  "aws-sdk-s3",
  "notify",
  "russh",
  "russh-sftp",
  "specta",
  "specta-typescript",
  "suppaftp",
  "tauri-specta",
]);

const INTERNAL_PACKAGES = new Set(REQUIRED_WORKSPACE_MEMBERS.map(({ packageName }) => packageName));

const ALLOWED_INTERNAL_DEPENDENCIES = {
  "serverus-domain": { normal: [], build: [], dev: ["serverus-testkit"] },
  "serverus-application": {
    normal: ["serverus-domain"],
    build: [],
    dev: ["serverus-domain", "serverus-testkit"],
  },
  "serverus-runtime": {
    normal: ["serverus-application", "serverus-domain"],
    build: [],
    dev: ["serverus-application", "serverus-domain", "serverus-testkit"],
  },
  "serverus-adapters": {
    normal: ["serverus-application", "serverus-domain"],
    build: [],
    dev: ["serverus-application", "serverus-domain", "serverus-testkit"],
  },
  "serverus-testkit": {
    normal: ["serverus-application", "serverus-domain", "serverus-runtime"],
    build: [],
    dev: ["serverus-application", "serverus-domain", "serverus-runtime"],
  },
  serverus: {
    normal: ["serverus-adapters", "serverus-application", "serverus-domain", "serverus-runtime"],
    build: [],
    dev: [
      "serverus-adapters",
      "serverus-application",
      "serverus-domain",
      "serverus-runtime",
      "serverus-testkit",
    ],
  },
};

function relativePath(root, path) {
  return relative(root, path).split(sep).join("/");
}

function dependencyKind(dependency) {
  return dependency.kind === null ? "normal" : dependency.kind;
}

function isTauriDependency(name) {
  return name === "tauri" || name.startsWith("tauri-");
}

function isProtocolOrIpcDependency(name) {
  return PROTOCOL_AND_IPC_DEPENDENCIES.has(name);
}

export function loadCargoMetadata(root) {
  const manifestPath = join(root, "Cargo.toml");
  if (!existsSync(manifestPath)) throw new Error(`Cargo workspace manifest not found: ${manifestPath}`);

  const result = spawnSync(
    process.env.CARGO ?? "cargo",
    ["metadata", "--format-version", "1", "--no-deps", "--manifest-path", manifestPath],
    { cwd: root, encoding: "utf8" },
  );
  if (result.error) throw new Error(`Unable to run cargo metadata: ${result.error.message}`);
  if (result.status !== 0) {
    const details = result.stderr.trim() || result.stdout.trim() || `exit code ${result.status}`;
    throw new Error(`cargo metadata failed:\n${details}`);
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(`cargo metadata returned invalid JSON: ${error.message}`);
  }
}

export function checkWorkspace(metadata, root) {
  const memberIds = new Set(metadata.workspace_members);
  const packages = metadata.packages.filter((pkg) => memberIds.has(pkg.id));
  const errors = [];

  for (const expected of REQUIRED_WORKSPACE_MEMBERS) {
    const pkg = packages.find(
      (candidate) =>
        candidate.name === expected.packageName &&
        relativePath(root, dirname(candidate.manifest_path)) === expected.path,
    );
    if (!pkg) {
      errors.push(
        `[workspace-member] Cargo workspace must include package "${expected.packageName}" ` +
          `at "${expected.path}". Add the member and ensure its Cargo.toml exists.`,
      );
    } else if (!existsSync(pkg.manifest_path)) {
      errors.push(
        `[workspace-member] Workspace package "${expected.packageName}" points to missing ` +
          `manifest ${relativePath(root, pkg.manifest_path)}.`,
      );
    }
  }
  return { errors, packages };
}

export function checkPackageDependencies(root, packages) {
  const rules = [
    {
      packageName: "serverus-domain",
      code: "rust-domain-dependency",
      forbidden: (name) =>
        ["serverus-application", "serverus-runtime", "serverus-adapters"].includes(name) ||
        name === "tokio" ||
        isTauriDependency(name) ||
        isProtocolOrIpcDependency(name),
      guidance: "Keep domain logic effect-free and move orchestration outward.",
    },
    {
      packageName: "serverus-application",
      code: "rust-application-dependency",
      forbidden: (name) =>
        ["serverus-runtime", "serverus-adapters"].includes(name) ||
        isTauriDependency(name) ||
        isProtocolOrIpcDependency(name),
      guidance: "Depend only inward on serverus-domain.",
    },
    {
      packageName: "serverus-runtime",
      code: "rust-runtime-dependency",
      forbidden: (name) =>
        name === "serverus-adapters" || isTauriDependency(name) || isProtocolOrIpcDependency(name),
      guidance: "Keep runtime ownership technology-neutral.",
    },
  ];
  const errors = [];
  for (const rule of rules) {
    const pkg = packages.find(({ name }) => name === rule.packageName);
    if (!pkg) continue;
    for (const dependency of pkg.dependencies.filter(({ name }) => rule.forbidden(name))) {
      const alias = dependency.rename ? ` (renamed to "${dependency.rename}")` : "";
      errors.push(
        `[${rule.code}] ${relativePath(root, pkg.manifest_path)}: ${rule.packageName} must not ` +
          `depend on "${dependency.name}"${alias} as a ${dependencyKind(dependency)} dependency. ` +
          rule.guidance,
      );
    }
  }
  return errors;
}

export function checkInternalDependencyDirection(root, packages) {
  const errors = [];
  for (const pkg of packages) {
    const rule = ALLOWED_INTERNAL_DEPENDENCIES[pkg.name];
    if (!rule) continue;
    for (const dependency of pkg.dependencies.filter(({ name }) => INTERNAL_PACKAGES.has(name))) {
      const kind = dependencyKind(dependency);
      if (new Set(rule[kind] ?? []).has(dependency.name)) continue;
      errors.push(
        `[rust-internal-dependency] ${relativePath(root, pkg.manifest_path)}: ` +
          `${pkg.name} must not depend on ${dependency.name} as a ${kind} dependency. ` +
          `Keep workspace edges inward-only; testkit is dev-only.`,
      );
    }
  }
  return errors;
}
