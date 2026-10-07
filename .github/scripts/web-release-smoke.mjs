import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const image = process.env.NYATERM_SMOKE_IMAGE;
assert(image, "NYATERM_SMOKE_IMAGE is required");
const name = "nyaterm-web-release-smoke";
const artifacts = resolve("artifacts/web-release-smoke");
mkdirSync(artifacts, { recursive: true });
const password = randomBytes(32).toString("base64");
const encryptionKey = randomBytes(32).toString("base64");
const docker = (args, optional = false) => {
  const result = spawnSync("docker", args, {
    encoding: "utf8",
    env: {
      ...process.env,
      NYATERM_WEB_PASSWORD: password,
      NYATERM_WEB_ENCRYPTION_KEY: encryptionKey,
    },
  });
  if (!optional && (result.error || result.status !== 0))
    throw new Error(`Docker ${args[0]} failed: ${result.stderr}`);
  return result;
};
const fetchWithTimeout = (url, options = {}) =>
  fetch(url, { ...options, signal: AbortSignal.timeout(5000) });

try {
  docker([
    "run",
    "-d",
    "--name",
    name,
    "--init",
    "-p",
    "127.0.0.1::8080",
    "-e",
    "NYATERM_WEB_PASSWORD",
    "-e",
    "NYATERM_WEB_ENCRYPTION_KEY",
    "--read-only",
    "--tmpfs",
    "/tmp",
    "--cap-drop",
    "ALL",
    "--security-opt",
    "no-new-privileges:true",
    image,
  ]);
  const binding = docker(["port", name, "8080/tcp"]).stdout.trim();
  const base = `http://${binding}/`;
  const deadline = Date.now() + 60_000;
  let response;
  while (Date.now() < deadline) {
    try {
      response = await fetchWithTimeout(base);
      if (response.ok) break;
    } catch {}
    await delay(500);
  }
  assert(response?.ok, "Container readiness timed out");
  const html = await response.text();
  assert(html.includes('<div id="root">'), "Missing application HTML");
  const assets = [...html.matchAll(/(?:src|href)="(\/assets\/[^"\s]+)"/g)];
  assert(assets.length > 0, "No built assets found");
  for (const [, path] of assets) {
    const asset = await fetchWithTimeout(new URL(path, base));
    assert.equal(asset.status, 200, `Asset unavailable: ${path}`);
    assert(
      !asset.headers.get("content-type")?.includes("text/html"),
      "Asset returned SPA fallback",
    );
    assert((await asset.arrayBuffer()).byteLength > 0, "Empty asset");
  }
  const sessionUrl = new URL("api/auth/session", base);
  assert.equal((await fetchWithTimeout(sessionUrl)).status, 401);
  const headers = {
    Origin: new URL(base).origin,
    "X-Nyaterm-Request": "1",
    "Content-Type": "application/json",
  };
  const loginUrl = new URL("api/auth/login", base);
  const rejected = await fetchWithTimeout(loginUrl, {
    method: "POST",
    headers,
    body: JSON.stringify({ password: "invalid" }),
  });
  assert.equal(rejected.status, 401);
  const login = await fetchWithTimeout(loginUrl, {
    method: "POST",
    headers,
    body: JSON.stringify({ password }),
  });
  assert.equal(login.status, 200, "Login failed");
  const { csrf } = await login.json();
  assert(csrf, "Missing CSRF token");
  const cookie = login.headers.get("set-cookie")?.split(";")[0];
  assert(cookie, "Missing session cookie");
  const session = await fetchWithTimeout(sessionUrl, {
    headers: { Cookie: cookie },
  });
  assert.equal(session.status, 200);
  assert.equal((await session.json()).csrf, csrf);
  console.log("Web release image smoke checks passed.");
} finally {
  const logs = docker(["logs", name], true);
  writeFileSync(resolve(artifacts, "web.log"), logs.stdout + logs.stderr);
  // -v removes the anonymous /data volume created by the Dockerfile.
  docker(["rm", "-f", "-v", name], true);
}
