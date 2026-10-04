import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { posix } from "node:path";
import ts from "typescript";

const serverRoot = "src-tauri/crates/nyaterm-web/src";
const server = new Set();
function literals(text) {
  return [...text.matchAll(/"([a-z][a-z0-9_]+)"/g)].map((match) => match[1]);
}
const commands = readFileSync(`${serverRoot}/commands.rs`, "utf8");
for (const match of commands.matchAll(
  /^ {8}((?:"[a-z][a-z0-9_]+"\s*\|\s*)*"[a-z][a-z0-9_]+")\s*=>/gm,
)) {
  for (const name of literals(match[1])) server.add(name);
}
for (const module of [
  ...commands.matchAll(/crate::(\w+)::supports\(command\)/g),
].map((match) => match[1])) {
  const source = readFileSync(`${serverRoot}/${module}.rs`, "utf8");
  const start = source.indexOf("pub fn supports(");
  const end = source.indexOf("\n}", start) + 2;
  if (start < 0 || end < 2)
    throw new Error(`Cannot locate ${module} allowlist`);
  for (const name of literals(source.slice(start, end))) server.add(name);
}
const http = ts.createSourceFile(
  "http.ts",
  readFileSync("src/lib/backend/http.ts", "utf8"),
  ts.ScriptTarget.Latest,
  true,
);
const browser = new Set();
function collectBrowser(node) {
  if (ts.isCaseClause(node) && ts.isStringLiteral(node.expression))
    browser.add(node.expression.text);
  if (
    ts.isCallExpression(node) &&
    ts.isPropertyAccessExpression(node.expression) &&
    node.expression.name.text === "includes" &&
    node.arguments[0]?.getText(http) === "command" &&
    ts.isArrayLiteralExpression(node.expression.expression)
  ) {
    for (const entry of node.expression.expression.elements)
      if (ts.isStringLiteral(entry)) browser.add(entry.text);
  }
  if (
    ts.isBinaryExpression(node) &&
    node.left.getText(http) === "command" &&
    ts.isStringLiteral(node.right) &&
    node.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken
  )
    browser.add(node.right.text);
  ts.forEachChild(node, collectBrowser);
}
collectBrowser(http);
const calls = new Map(),
  dynamic = [];
function walk(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true }).sort(
    (a, b) => a.name.localeCompare(b.name, "en"),
  )) {
    const file = posix.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (file !== "src/test") walk(file);
      continue;
    }
    if (!/\.tsx?$/.test(file) || /\.(test|spec)\.tsx?$/.test(file)) continue;
    const source = ts.createSourceFile(
      file,
      readFileSync(file, "utf8"),
      ts.ScriptTarget.Latest,
      true,
    );
    const visit = (node) => {
      if (
        ts.isCallExpression(node) &&
        ts.isIdentifier(node.expression) &&
        /^(invoke|tauriInvoke|httpInvoke)$/.test(node.expression.text) &&
        node.arguments[0]
      ) {
        const arg = node.arguments[0],
          location = {
            file,
            line:
              source.getLineAndCharacterOfPosition(node.getStart(source)).line +
              1,
          };
        const add = (name) => {
          if (!calls.has(name)) calls.set(name, []);
          calls.get(name).push(location);
        };
        if (ts.isStringLiteralLike(arg)) add(arg.text);
        else {
          dynamic.push({ ...location, expression: arg.getText(source) });
          // Conditional literals are enumerated while retaining the dynamic call for review.
          if (ts.isConditionalExpression(arg))
            for (const branch of [arg.whenTrue, arg.whenFalse])
              if (ts.isStringLiteralLike(branch)) add(branch.text);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
}
walk("src");
const entries = [...calls]
  .sort(([a], [b]) => a.localeCompare(b, "en"))
  .map(([command, locations]) => ({
    command,
    boundary: browser.has(command)
      ? "browser-adapter"
      : server.has(command)
        ? "server-allowlist"
        : "desktop-or-unmatched",
    locations,
  }));
const inventory = {
  schemaVersion: 2,
  generator: "pnpm web:audit",
  scope:
    "Production frontend invoke calls, conditional literal branches, browser adapter and explicit server allowlists. Static evidence only; capability-gated desktop calls are not Web defects.",
  counts: Object.fromEntries(
    ["browser-adapter", "server-allowlist", "desktop-or-unmatched"].map(
      (kind) => [
        kind,
        entries.filter((entry) => entry.boundary === kind).length,
      ],
    ),
  ),
  serverCommands: [...server].sort(),
  browserCommands: [...browser].sort(),
  entries,
  dynamic,
};
const output = `${JSON.stringify(inventory, null, 2)}\n`,
  destination = "docs/web-api-inventory.json";
if (process.argv.includes("--check")) {
  if (readFileSync(destination, "utf8") !== output)
    throw new Error("Web inventory is stale. Run pnpm web:audit.");
} else writeFileSync(destination, output);
console.log(JSON.stringify(inventory.counts));
