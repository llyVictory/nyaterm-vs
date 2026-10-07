import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";

const listener = createServer();
await new Promise((resolve) => listener.listen(0, "127.0.0.1", resolve));
const port = listener.address().port;
await new Promise((resolve) => listener.close(resolve));
const data = mkdtempSync(join(tmpdir(), "nyaterm-local-e2e-"));
const artifacts = resolve(
  "artifacts/web-local-e2e",
  process.env.NYATERM_LOCAL_E2E_BASE_PATH ? "subpath" : "root",
);
mkdirSync(artifacts, { recursive: true });
const basePath = process.env.NYATERM_LOCAL_E2E_BASE_PATH ?? "/";
const base = `http://127.0.0.1:${port}${basePath}`;
const password = randomBytes(32).toString("base64");
const env = {
  ...process.env,
  NYATERM_WEB_PASSWORD: password,
  NYATERM_WEB_ENCRYPTION_KEY: randomBytes(32).toString("base64"),
  NYATERM_WEB_DATA_DIR: data,
  NYATERM_WEB_DIST: resolve("dist"),
  NYATERM_WEB_BASE_PATH: basePath,
  NYATERM_WEB_BIND: `127.0.0.1:${port}`,
};
delete env.NYATERM_WEB_PASSWORD_FILE;
delete env.NYATERM_WEB_ENCRYPTION_KEY_FILE;
const exe = resolve(
  "src-tauri/crates/nyaterm-web/target/debug/nyaterm-web" +
    (process.platform === "win32" ? ".exe" : ""),
);
const server = spawn(exe, [], {
  env,
  stdio: ["ignore", "ignore", "inherit"],
  windowsHide: true,
});
let serverError;
server.on("error", (error) => {
  serverError = error;
});
try {
  let ready = false;
  for (let attempt = 0; attempt < 200; attempt++) {
    if (serverError) throw serverError;
    if (server.exitCode !== null)
      throw new Error("Local Web server exited during startup");
    try {
      if ((await fetch(base)).ok) {
        ready = true;
        break;
      }
    } catch {}
    await delay(50);
  }
  if (!ready) throw new Error("Local Web server was not ready");
  const test = spawn(
    process.execPath,
    [
      "node_modules/@playwright/test/cli.js",
      "test",
      "--config",
      "e2e/playwright.config.mjs",
    ],
    {
      env: {
        ...process.env,
        NYATERM_E2E_FEATURES_ONLY: "1",
        NYATERM_E2E_URL: base,
        NYATERM_E2E_PASSWORD: password,
        NYATERM_E2E_ARTIFACTS: artifacts,
      },
      stdio: "inherit",
      windowsHide: true,
    },
  );
  const code = await new Promise((resolve, reject) => {
    test.once("error", reject);
    test.once("exit", resolve);
  });
  if (code !== 0) process.exitCode = 1;
} finally {
  if (server.exitCode === null && !serverError) {
    const exited = new Promise((resolve) => server.once("exit", resolve));
    server.kill();
    await exited;
  }
  rmSync(data, { recursive: true, force: true });
}
