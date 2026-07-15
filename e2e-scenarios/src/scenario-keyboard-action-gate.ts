import ts from "typescript";

const APPROVED_HELPER_LABEL = "support/keyboard.ts";
const APPROVED_HELPER_FUNCTION = "pressPrimaryShortcut";

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

function isBrowserExpression(expression: ts.Expression): boolean {
  if (ts.isIdentifier(expression)) {
    const isPropertyName =
      ts.isPropertyAccessExpression(expression.parent) && expression.parent.name === expression;
    return expression.text === "browser" && !isPropertyName;
  }
  const receiver = accessReceiver(expression);
  return (
    accessedProperty(expression) === "browser" &&
    receiver !== null &&
    ts.isIdentifier(receiver) &&
    receiver.text === "globalThis"
  );
}

function isBrowserMethod(expression: ts.Expression, method: string): boolean {
  const receiver = accessReceiver(expression);
  return accessedProperty(expression) === method && receiver !== null && isBrowserExpression(receiver);
}

function enclosingFunction(node: ts.Node): ts.FunctionDeclaration | null {
  let current: ts.Node | undefined = node.parent;
  while (current) {
    if (ts.isFunctionDeclaration(current)) return current;
    if (ts.isFunctionLike(current)) return null;
    current = current.parent;
  }
  return null;
}

function chainedCall(
  receiver: ts.CallExpression,
  method: string,
  argument?: string,
): ts.CallExpression | null {
  const access = receiver.parent;
  if (
    !ts.isPropertyAccessExpression(access) ||
    access.expression !== receiver ||
    access.name.text !== method
  ) {
    return null;
  }
  const call = access.parent;
  if (!ts.isCallExpression(call) || call.expression !== access) return null;
  if (argument === undefined) return call.arguments.length === 0 ? call : null;
  return call.arguments.length === 1 &&
    ts.isIdentifier(call.arguments[0]) &&
    call.arguments[0].text === argument
    ? call
    : null;
}

function isExactPrimaryShortcutAction(node: ts.CallExpression): boolean {
  const modifierDown = chainedCall(node, "down", "primaryModifier");
  const keyDown = modifierDown && chainedCall(modifierDown, "down", "key");
  const keyUp = keyDown && chainedCall(keyDown, "up", "key");
  const modifierUp = keyUp && chainedCall(keyUp, "up", "primaryModifier");
  const perform = modifierUp && chainedCall(modifierUp, "perform");
  return Boolean(
    perform &&
      ts.isAwaitExpression(perform.parent) &&
      ts.isExpressionStatement(perform.parent.parent),
  );
}

function browserActionCount(node: ts.Node): number {
  let count = 0;
  function visit(child: ts.Node): void {
    if (ts.isCallExpression(child) && isBrowserMethod(child.expression, "action")) count += 1;
    ts.forEachChild(child, visit);
  }
  visit(node);
  return count;
}

function isApprovedHelper(node: ts.CallExpression, label: string): boolean {
  const declaration = enclosingFunction(node);
  return Boolean(
    label === APPROVED_HELPER_LABEL &&
      declaration?.name?.text === APPROVED_HELPER_FUNCTION &&
      declaration.body &&
      browserActionCount(declaration.body) === 1 &&
      isExactPrimaryShortcutAction(node),
  );
}

function browserActionViolation(node: ts.CallExpression): string | null {
  if (!isBrowserMethod(node.expression, "action")) return null;
  const kind = node.arguments[0];
  if (!kind || !ts.isStringLiteralLike(kind)) return "dynamic browser.action()";
  return kind.text === "key" ? "raw keyboard action()" : null;
}

function isSingleLiteralKey(expression: ts.Expression): boolean {
  if (ts.isStringLiteralLike(expression)) return true;
  if (!ts.isPropertyAccessExpression(expression) && !ts.isElementAccessExpression(expression)) {
    return false;
  }
  return ts.isIdentifier(expression.expression) && expression.expression.text === "Key";
}

function isRawKeysChord(node: ts.CallExpression): boolean {
  return (
    isBrowserMethod(node.expression, "keys") &&
    (node.arguments.length !== 1 || !isSingleLiteralKey(node.arguments[0]))
  );
}

export function forbiddenRawKeyboardAction(node: ts.Node, label: string): string | null {
  if (ts.isCallExpression(node)) {
    const actionViolation = browserActionViolation(node);
    if (
      actionViolation &&
      !(actionViolation === "raw keyboard action()" && isApprovedHelper(node, label))
    ) {
      return actionViolation;
    }
    if (isRawKeysChord(node)) return "raw browser.keys chord";
  }
  if (
    ts.isExpression(node) &&
    isBrowserExpression(node) &&
    !(
      (ts.isPropertyAccessExpression(node.parent) || ts.isElementAccessExpression(node.parent)) &&
      node.parent.expression === node
    )
  ) {
    return "browser alias";
  }
  if (
    (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) &&
    (isBrowserMethod(node, "action") || isBrowserMethod(node, "keys")) &&
    !(ts.isCallExpression(node.parent) && node.parent.expression === node)
  ) {
    return `browser.${accessedProperty(node)} reference`;
  }
  return null;
}
