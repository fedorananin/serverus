import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import ts from "typescript";

const ASSIGNMENT_OPERATORS = new Set<ts.SyntaxKind>([
  ts.SyntaxKind.EqualsToken,
  ts.SyntaxKind.PlusEqualsToken,
  ts.SyntaxKind.MinusEqualsToken,
  ts.SyntaxKind.AsteriskEqualsToken,
  ts.SyntaxKind.AsteriskAsteriskEqualsToken,
  ts.SyntaxKind.SlashEqualsToken,
  ts.SyntaxKind.PercentEqualsToken,
  ts.SyntaxKind.LessThanLessThanEqualsToken,
  ts.SyntaxKind.GreaterThanGreaterThanEqualsToken,
  ts.SyntaxKind.GreaterThanGreaterThanGreaterThanEqualsToken,
  ts.SyntaxKind.AmpersandEqualsToken,
  ts.SyntaxKind.BarEqualsToken,
  ts.SyntaxKind.CaretEqualsToken,
  ts.SyntaxKind.BarBarEqualsToken,
  ts.SyntaxKind.AmpersandAmpersandEqualsToken,
  ts.SyntaxKind.QuestionQuestionEqualsToken,
]);

function accessedProperty(expression: ts.Expression): string | null {
  if (ts.isPropertyAccessExpression(expression)) return expression.name.text;
  if (
    ts.isElementAccessExpression(expression) &&
    expression.argumentExpression &&
    ts.isStringLiteralLike(expression.argumentExpression)
  ) {
    return expression.argumentExpression.text;
  }
  return null;
}

function accessReceiver(expression: ts.Expression): ts.Expression | null {
  if (ts.isPropertyAccessExpression(expression) || ts.isElementAccessExpression(expression)) {
    return expression.expression;
  }
  return null;
}

function propertyName(name: ts.PropertyName): string | null {
  if (ts.isIdentifier(name) || ts.isStringLiteralLike(name)) return name.text;
  return null;
}

function isRightButtonValue(expression: ts.Expression): boolean {
  return (
    (ts.isStringLiteralLike(expression) && expression.text === "right") ||
    (ts.isNumericLiteral(expression) && Number(expression.text) === 2)
  );
}

function isDomType(type: ts.TypeNode | undefined): boolean {
  if (!type) return false;
  if (ts.isParenthesizedTypeNode(type)) return isDomType(type.type);
  if (ts.isUnionTypeNode(type) || ts.isIntersectionTypeNode(type)) {
    return type.types.some(isDomType);
  }
  if (!ts.isTypeReferenceNode(type) || !ts.isIdentifier(type.typeName)) return false;
  return /^(?:Element|HTMLElement|HTML[A-Za-z]*Element|SVG[A-Za-z]*Element)$/.test(
    type.typeName.text,
  );
}

type BindingDeclaration = ts.VariableDeclaration | ts.ParameterDeclaration;
type BindingScope = ts.SourceFile | ts.Block | ts.SignatureDeclaration;

function isBindingScope(node: ts.Node): node is BindingScope {
  return ts.isSourceFile(node) || ts.isBlock(node) || ts.isFunctionLike(node);
}

function bindingScope(declaration: BindingDeclaration): BindingScope | null {
  let current: ts.Node | undefined = declaration.parent;
  while (current) {
    if (ts.isParameter(declaration)) {
      if (ts.isFunctionLike(current)) return current;
    } else if (isBindingScope(current)) {
      return current;
    }
    current = current.parent;
  }
  return null;
}

function collectBindings(file: ts.SourceFile): Map<BindingScope, Map<string, BindingDeclaration>> {
  const scopes = new Map<BindingScope, Map<string, BindingDeclaration>>();
  function visit(node: ts.Node): void {
    if (
      (ts.isVariableDeclaration(node) || ts.isParameter(node)) &&
      ts.isIdentifier(node.name)
    ) {
      const scope = bindingScope(node);
      if (scope) {
        const bindings = scopes.get(scope) ?? new Map<string, BindingDeclaration>();
        bindings.set(node.name.text, node);
        scopes.set(scope, bindings);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  return scopes;
}

function isDomExpression(
  expression: ts.Expression,
  isDomIdentifier: (identifier: ts.Identifier) => boolean,
): boolean {
  if (ts.isIdentifier(expression)) {
    return expression.text === "document" || isDomIdentifier(expression);
  }
  if (
    ts.isParenthesizedExpression(expression) ||
    ts.isAsExpression(expression) ||
    ts.isTypeAssertionExpression(expression) ||
    ts.isNonNullExpression(expression)
  ) {
    if (
      (ts.isAsExpression(expression) || ts.isTypeAssertionExpression(expression)) &&
      isDomType(expression.type)
    ) {
      return true;
    }
    return isDomExpression(expression.expression, isDomIdentifier);
  }
  if (ts.isPropertyAccessExpression(expression) || ts.isElementAccessExpression(expression)) {
    return (
      accessedProperty(expression) === "document" ||
      isDomExpression(expression.expression, isDomIdentifier)
    );
  }
  if (ts.isCallExpression(expression)) {
    const receiver = accessReceiver(expression.expression);
    return receiver !== null && isDomExpression(receiver, isDomIdentifier);
  }
  return false;
}

function forbiddenInputOperations(source: string, label: string): string[] {
  const file = ts.createSourceFile(label, source, ts.ScriptTarget.Latest, true);
  const scopes = collectBindings(file);
  const domBindings = new Map<BindingDeclaration, boolean>();
  const resolvingBindings = new Set<BindingDeclaration>();
  const errors: string[] = [];

  function resolveBinding(identifier: ts.Identifier): BindingDeclaration | null {
    let current: ts.Node | undefined = identifier;
    while (current) {
      if (isBindingScope(current)) {
        const declaration = scopes.get(current)?.get(identifier.text);
        if (declaration) return declaration;
      }
      current = current.parent;
    }
    return null;
  }

  function isDomIdentifier(identifier: ts.Identifier): boolean {
    const declaration = resolveBinding(identifier);
    if (!declaration) return false;
    const cached = domBindings.get(declaration);
    if (cached !== undefined) return cached;
    if (resolvingBindings.has(declaration)) return false;
    resolvingBindings.add(declaration);
    const result =
      isDomType(declaration.type) ||
      Boolean(
        declaration.initializer && isDomExpression(declaration.initializer, isDomIdentifier),
      );
    resolvingBindings.delete(declaration);
    domBindings.set(declaration, result);
    return result;
  }

  function report(node: ts.Node, operation: string): void {
    const line = file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1;
    errors.push(`${label}:${line}: ${operation} is forbidden in real-input scenarios`);
  }

  function visit(node: ts.Node, browserExecution = false): void {
    let insideBrowserExecution = browserExecution;
    if (ts.isCallExpression(node)) {
      const method = accessedProperty(node.expression);
      const receiver = accessReceiver(node.expression);
      const executionMethod =
        method === "execute" || method === "executeAsync"
          ? method
          : ts.isIdentifier(node.expression) &&
              (node.expression.text === "execute" || node.expression.text === "executeAsync")
            ? node.expression.text
            : null;

      if (executionMethod !== null) {
        const directBrowser =
          receiver !== null && ts.isIdentifier(receiver) && receiver.text === "browser";
        report(node, directBrowser ? `browser.${executionMethod}` : `${executionMethod}()`);
        insideBrowserExecution = true;
      } else if (method === "retries") {
        report(node, "retries()");
      } else if (method === "dispatchEvent" || method === "requestSubmit") {
        report(node, method);
      } else if (
        method === "click" &&
        receiver !== null &&
        (insideBrowserExecution || isDomExpression(receiver, isDomIdentifier))
      ) {
        report(node, "DOM click()");
      } else if (
        (method === "click" || method === "down" || method === "up") &&
        node.arguments[0] &&
        isRightButtonValue(node.arguments[0])
      ) {
        report(node, "right-button automation");
      }
    }

    if (
      ts.isPropertyAssignment(node) &&
      propertyName(node.name) === "button" &&
      isRightButtonValue(node.initializer)
    ) {
      report(node, "right-button automation");
    }

    if (
      (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) &&
      (accessedProperty(node) === "execute" || accessedProperty(node) === "executeAsync") &&
      !(ts.isCallExpression(node.parent) && node.parent.expression === node)
    ) {
      report(node, `${accessedProperty(node)} reference`);
    }

    if (
      ts.isBinaryExpression(node) &&
      ASSIGNMENT_OPERATORS.has(node.operatorToken.kind) &&
      accessedProperty(node.left) === "value"
    ) {
      report(node, "direct .value assignment");
    }

    node.forEachChild((child) => visit(child, insideBrowserExecution));
  }

  visit(file);
  return errors;
}

function sourceFiles(root: string): string[] {
  if (!existsSync(root)) return [];
  const files: string[] = [];
  const entries = readdirSync(root, { withFileTypes: true }).sort((a, b) =>
    a.name.localeCompare(b.name),
  );
  for (const entry of entries) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) files.push(...sourceFiles(path));
    else if (entry.isFile() && path.endsWith(".ts")) files.push(path);
  }
  return files;
}

function portablePath(path: string): string {
  return path.split(sep).join("/");
}

export function validateRealInputSources(root: string, catalog: readonly string[]): string[] {
  const errors: string[] = [];
  for (const id of catalog) {
    for (const path of sourceFiles(join(root, id))) {
      const label = portablePath(relative(root, path));
      errors.push(...forbiddenInputOperations(readFileSync(path, "utf8"), label));
    }
  }

  if (catalog.length > 0) {
    const supportRoot = join(dirname(root), "support");
    for (const path of sourceFiles(supportRoot)) {
      const label = `support/${portablePath(relative(supportRoot, path))}`;
      errors.push(...forbiddenInputOperations(readFileSync(path, "utf8"), label));
    }
  }
  return errors;
}
