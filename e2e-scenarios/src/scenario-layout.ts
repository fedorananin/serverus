import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import ts from "typescript";

import { validateRealInputSources } from "./scenario-input-gate";

function entryContracts(source: string, id: string): { taggedSuite: boolean; executableTests: number } {
  const file = ts.createSourceFile("scenario.e2e.spec.ts", source, ts.ScriptTarget.Latest, true);
  let taggedSuite = false;
  let executableTests = 0;

  function countDirectTests(callback: ts.Node): number {
    if (!ts.isArrowFunction(callback) && !ts.isFunctionExpression(callback)) return 0;
    if (!ts.isBlock(callback.body)) return 0;
    return callback.body.statements.filter((statement) => {
      if (!ts.isExpressionStatement(statement) || !ts.isCallExpression(statement.expression)) {
        return false;
      }
      const call = statement.expression;
      return (
        ts.isIdentifier(call.expression) &&
        (call.expression.text === "it" || call.expression.text === "test") &&
        call.arguments.length >= 2
      );
    }).length;
  }

  function visit(node: ts.Node): void {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)) {
      const name = node.expression.text;
      const [title] = node.arguments;
      if (name === "describe" && title && ts.isStringLiteralLike(title) && title.text === `@${id}`) {
        taggedSuite = true;
        const suiteBody = node.arguments[1];
        if (suiteBody) executableTests += countDirectTests(suiteBody);
      }
    }
    ts.forEachChild(node, visit);
  }

  visit(file);
  return { taggedSuite, executableTests };
}

function entrySpecs(root: string, prefix = ""): string[] {
  if (!existsSync(root)) return [];
  const specs: string[] = [];
  const entries = readdirSync(root, { withFileTypes: true }).sort((a, b) =>
    a.name.localeCompare(b.name),
  );
  for (const entry of entries) {
    const path = join(root, entry.name);
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) specs.push(...entrySpecs(path, relative));
    else if (entry.isFile() && entry.name.endsWith(".e2e.spec.ts")) specs.push(relative);
  }
  return specs;
}

export function validateScenarioLayout(root: string, catalog: readonly string[]): string[] {
  const errors: string[] = [];
  const registered = new Set(catalog);

  for (const id of catalog) {
    const directory = join(root, id);
    const entry = join(directory, `${id}.e2e.spec.ts`);
    if (!existsSync(entry)) {
      errors.push(`${id}: missing ${id}.e2e.spec.ts`);
    } else {
      const contracts = entryContracts(readFileSync(entry, "utf8"), id);
      if (!contracts.taggedSuite) {
        errors.push(`${id}: entry spec must declare describe("@${id}", ...)`);
      }
      if (contracts.taggedSuite && contracts.executableTests === 0) {
        errors.push(`${id}: entry spec must contain at least one direct executable test`);
      }
    }
  }

  if (!existsSync(root)) return errors;
  for (const spec of entrySpecs(root)) {
    const [owner, ...rest] = spec.split("/");
    if (registered.has(owner)) {
      if (spec !== `${owner}/${owner}.e2e.spec.ts`) {
        errors.push(`${owner}: unexpected additional entry spec ${rest.join("/")}`);
      }
    } else if (rest.length === 0) {
      errors.push(`${spec}: entry spec is outside a registered scenario directory`);
    }
  }
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (entry.isDirectory() && !registered.has(entry.name)) {
      errors.push(`${entry.name}: scenario directory is not registered`);
    }
  }

  errors.push(...validateRealInputSources(root, catalog));

  return errors;
}
