#!/usr/bin/env node
/* global process, console */
/**
 * Add an opt-in foregroundIntense theme color for bold text that still uses
 * xterm's default foreground color.
 *
 * This script is pinned to @xterm/xterm@6.1.0-beta.288 and
 * @xterm/addon-webgl@0.20.0-beta.287. It is idempotent and fails fast when
 * either minified bundle no longer matches the expected fragments.
 */

"use strict";

const fs = require("node:fs");
const path = require("node:path");

const CORE_PACKAGE = "@xterm/xterm";
const CORE_VERSION = "6.1.0-beta.288";
const CORE_MARKER = "/*nyaterm:foreground-intense-core*/";
const WEBGL_PACKAGE = "@xterm/addon-webgl";
const WEBGL_VERSION = "0.20.0-beta.287";
const WEBGL_MARKER = "/*nyaterm:foreground-intense-webgl*/";

const CORE_DIR = path.resolve(process.cwd(), "node_modules", "@xterm", "xterm");
const WEBGL_DIR = path.resolve(
  process.cwd(),
  "node_modules",
  "@xterm",
  "addon-webgl",
);

const TARGETS = [
  {
    marker: CORE_MARKER,
    file: path.join(CORE_DIR, "lib", "xterm.mjs"),
    replacements: [
      {
        label: "ESM theme foregroundIntense parsing",
        original:
          "t.foreground=M(e.foreground,qe),t.background=M(e.background,Qt)",
        patched: `${CORE_MARKER}t.foreground=M(e.foreground,qe),t.foregroundIntense=e.foregroundIntense===void 0?void 0:M(e.foregroundIntense,t.foreground),t.background=M(e.background,Qt)`,
      },
      {
        label: "ESM DOM default foreground rendering",
        original:
          "case 0:default:this._applyMinimumContrast(I,Ae,f.foreground,x,Le,pi)||Br&&N.push(`xterm-fg-${257}`)}",
        patched:
          "case 0:default:{let A=x.isBold()&&!Br&&f.foregroundIntense?f.foregroundIntense:f.foreground;this._applyMinimumContrast(I,Ae,A,x,Le,pi)||(x.isBold()&&!Br&&f.foregroundIntense?this._addStyle(I,`color:${(x.isDim()?k.multiplyOpacity(f.foregroundIntense,.5):f.foregroundIntense).css}`):Br&&N.push(`xterm-fg-${257}`))}}",
      },
    ],
  },
  {
    marker: CORE_MARKER,
    file: path.join(CORE_DIR, "lib", "xterm.js"),
    replacements: [
      {
        label: "CJS theme foregroundIntense parsing",
        original:
          "t.foreground=m(e.foreground,d),t.background=m(e.background,_)",
        patched: `${CORE_MARKER}t.foreground=m(e.foreground,d),t.foregroundIntense=e.foregroundIntense===void 0?void 0:m(e.foregroundIntense,t.foreground),t.background=m(e.background,_)`,
      },
      {
        label: "CJS DOM default foreground rendering",
        original:
          "default:this._applyMinimumContrast(w,Q,b.foreground,W,J,Z)||G&&P.push(`xterm-fg-${o.INVERTED_DEFAULT_COLOR}`)}",
        patched:
          "default:{const e=W.isBold()&&!G&&b.foregroundIntense?b.foregroundIntense:b.foreground;this._applyMinimumContrast(w,Q,e,W,J,Z)||(W.isBold()&&!G&&b.foregroundIntense?this._addStyle(w,`color:${(W.isDim()?l.color.multiplyOpacity(b.foregroundIntense,.5):b.foregroundIntense).css}`):G&&P.push(`xterm-fg-${o.INVERTED_DEFAULT_COLOR}`))}}",
      },
    ],
  },
  {
    marker: WEBGL_MARKER,
    file: path.join(WEBGL_DIR, "lib", "addon-webgl.mjs"),
    replacements: [
      {
        label: "ESM WebGL foregroundIntense config",
        original: "let T={foreground:a.foreground,background:a.background",
        patched: `let T={${WEBGL_MARKER}foreground:a.foreground,foregroundIntense:a.foregroundIntense,background:a.background`,
      },
      {
        label: "ESM WebGL foregroundIntense config equality",
        original:
          "i.colors.foreground.rgba===e.colors.foreground.rgba&&i.colors.background.rgba===e.colors.background.rgba",
        patched:
          "i.colors.foreground.rgba===e.colors.foreground.rgba&&i.colors.foregroundIntense?.rgba===e.colors.foregroundIntense?.rgba&&i.colors.background.rgba===e.colors.background.rgba",
      },
      {
        label: "ESM WebGL default foreground rendering",
        original:
          "case 0:default:n?_=this._config.colors.background:_=this._config.colors.foreground}",
        patched:
          "case 0:default:n?_=this._config.colors.background:T&&this._config.colors.foregroundIntense?_=this._config.colors.foregroundIntense:_=this._config.colors.foreground}",
      },
      {
        label: "ESM WebGL minimum contrast foreground resolution",
        original:
          "case 0:default:return r?this._config.colors.background.rgba:this._config.colors.foreground.rgba}",
        patched:
          "case 0:default:return r?this._config.colors.background.rgba:s&&this._config.colors.foregroundIntense?this._config.colors.foregroundIntense.rgba:this._config.colors.foreground.rgba}",
      },
    ],
  },
  {
    marker: WEBGL_MARKER,
    file: path.join(WEBGL_DIR, "lib", "addon-webgl.js"),
    replacements: [
      {
        label: "CJS WebGL foregroundIntense config",
        original: "const T={foreground:o.foreground,background:o.background",
        patched: `const T={${WEBGL_MARKER}foreground:o.foreground,foregroundIntense:o.foregroundIntense,background:o.background`,
      },
      {
        label: "CJS WebGL foregroundIntense config equality",
        original:
          "t.colors.foreground.rgba===e.colors.foreground.rgba&&t.colors.background.rgba===e.colors.background.rgba",
        patched:
          "t.colors.foreground.rgba===e.colors.foreground.rgba&&t.colors.foregroundIntense?.rgba===e.colors.foregroundIntense?.rgba&&t.colors.background.rgba===e.colors.background.rgba",
      },
      {
        label: "CJS WebGL default foreground rendering",
        original:
          "default:_=L?this._config.colors.background:this._config.colors.foreground}",
        patched:
          "default:_=L?this._config.colors.background:l&&this._config.colors.foregroundIntense?this._config.colors.foregroundIntense:this._config.colors.foreground}",
      },
      {
        label: "CJS WebGL minimum contrast foreground resolution",
        original:
          "default:return i?this._config.colors.background.rgba:this._config.colors.foreground.rgba}",
        patched:
          "default:return i?this._config.colors.background.rgba:s&&this._config.colors.foregroundIntense?this._config.colors.foregroundIntense.rgba:this._config.colors.foreground.rgba}",
      },
    ],
  },
];

function fail(message) {
  throw new Error(`[patch-xterm-foreground-intense] ${message}`);
}

function countOccurrences(source, needle) {
  return source.split(needle).length - 1;
}

function writeFileAtomically(filename, content) {
  const temporary = `${filename}.nyaterm-patch-${process.pid}.tmp`;
  fs.writeFileSync(temporary, content, "utf8");
  fs.renameSync(temporary, filename);
}

function verifyPackage(packageName, packageDir, expectedVersion) {
  const packageJsonPath = path.join(packageDir, "package.json");
  if (!fs.existsSync(packageJsonPath)) {
    fail(`${packageName} is not installed: ${packageJsonPath}`);
  }

  let packageJson;
  try {
    packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  } catch (error) {
    fail(
      `cannot read ${packageJsonPath}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }

  if (packageJson.version !== expectedVersion) {
    fail(
      `unsupported ${packageName} version ${JSON.stringify(packageJson.version)}; ` +
        `expected ${expectedVersion}. Review and refresh this patch before upgrading.`,
    );
  }
}

function verifyPatchedSource(source, relative, marker, replacements) {
  const markerCount = countOccurrences(source, marker);
  if (markerCount !== 1) {
    fail(
      `expected exactly one patch marker in ${relative}, found ${markerCount}`,
    );
  }

  for (const replacement of replacements) {
    const originalCount = countOccurrences(source, replacement.original);
    const patchedCount = countOccurrences(source, replacement.patched);
    if (originalCount !== 0 || patchedCount !== 1) {
      fail(
        `patch verification failed for ${replacement.label} in ${relative}: ` +
          `original=${originalCount} patched=${patchedCount}`,
      );
    }
  }
}

verifyPackage(CORE_PACKAGE, CORE_DIR, CORE_VERSION);
verifyPackage(WEBGL_PACKAGE, WEBGL_DIR, WEBGL_VERSION);

let patched = 0;
let already = 0;

for (const { file, marker, replacements } of TARGETS) {
  const relative = path.relative(process.cwd(), file);
  if (!fs.existsSync(file)) {
    fail(`runtime bundle not found: ${relative}`);
  }

  let source;
  try {
    source = fs.readFileSync(file, "utf8");
  } catch (error) {
    fail(
      `cannot read ${relative}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }

  if (source.includes(marker)) {
    verifyPatchedSource(source, relative, marker, replacements);
    already += 1;
    continue;
  }

  for (const replacement of replacements) {
    const occurrences = countOccurrences(source, replacement.original);
    if (occurrences !== 1) {
      fail(
        `expected exactly one ${replacement.label} fragment in ${relative}, found ${occurrences}. ` +
          "The minified xterm bundle may have changed.",
      );
    }
    if (source.includes(replacement.patched)) {
      fail(`unexpected partial patch for ${replacement.label} in ${relative}`);
    }
  }

  let patchedSource = source;
  for (const replacement of replacements) {
    patchedSource = patchedSource.replace(
      replacement.original,
      replacement.patched,
    );
  }
  verifyPatchedSource(patchedSource, relative, marker, replacements);

  try {
    writeFileAtomically(file, patchedSource);
  } catch (error) {
    fail(
      `cannot write ${relative}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  patched += 1;
}

console.log(
  `[patch-xterm-foreground-intense] default foreground highlighting: ` +
    `patched=${patched} already=${already} total=${TARGETS.length}`,
);
