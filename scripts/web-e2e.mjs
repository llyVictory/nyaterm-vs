import { spawnSync } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { resolve, join, relative, basename } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const id = `nyaterm-e2e-${randomUUID().slice(0, 8)}`;
const secrets = mkdtempSync(join(tmpdir(), `${id}-`));
const artifacts = resolve("artifacts/web-e2e");
mkdirSync(artifacts, { recursive: true });
const password = randomBytes(32).toString("base64");
const sshPassword = randomBytes(32).toString("base64");
for (const [name, value] of Object.entries({
  login: password,
  encryption: randomBytes(32).toString("base64"),
  "ssh-password": sshPassword,
})) {
  writeFileSync(join(secrets, name), value, { mode: 0o644 });
}
chmodSync(secrets, 0o755); // Container UID 10001 must read only these disposable credentials.
const docker = (args, capture = false, optional = false) => {
  const result = spawnSync("docker", args, {
    encoding: "utf8",
    stdio: capture ? "pipe" : "inherit",
  });
  if (!optional && (result.error || result.status !== 0))
    throw new Error(`Docker ${args[0]} failed`);
  return (
    (result.stdout ?? "") + (args[0] === "logs" ? (result.stderr ?? "") : "")
  );
};
const mount = (name, target) => [
  "--mount",
  `type=bind,src=${join(secrets, name)},dst=/run/secrets/${target},readonly`,
];
const failures = [];
const containers = [],
  volumes = [],
  images = [];
let networkCreated = false;
async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return port;
}
try {
  docker(["version"], true);
  docker(["network", "create", id]);
  networkCreated = true;
  docker(["build", "-t", `${id}-ssh`, "e2e/ssh"]);
  images.push(`${id}-ssh`);
  for (const [name, basePath] of [
    ["root", "/"],
    ["subpath", "/nyaterm/"],
  ]) {
    const output = join(artifacts, name);
    mkdirSync(output, { recursive: true });
    const image = `${id}-${name}`;
    docker([
      "build",
      "--build-arg",
      `NYATERM_WEB_BASE_PATH=${basePath}`,
      "-t",
      image,
      ".",
    ]);
    images.push(image);
    const ssh = `${id}-${name}-ssh`,
      web = `${id}-${name}-web`;
    const sshVolume = `${ssh}-data`,
      webVolume = `${web}-data`;
    for (const volume of [sshVolume, webVolume]) {
      docker(["volume", "create", volume]);
      volumes.push(volume);
    }
    containers.push(ssh, web);
    docker([
      "run",
      "-d",
      "--name",
      ssh,
      "--network",
      id,
      "--network-alias",
      "ssh-fixture",
      ...mount("ssh-password", "ssh-password"),
      "--mount",
      `type=volume,src=${sshVolume},dst=/home/test`,
      `${id}-ssh`,
    ]);
    const port = await freePort();
    const url = `http://localhost:${port}${basePath}`;
    docker([
      "run",
      "-d",
      "--init",
      "--name",
      web,
      "--network",
      id,
      "-p",
      `127.0.0.1:${port}:8080`,
      "-e",
      `NYATERM_WEB_PUBLIC_URL=${url}`,
      "-e",
      "NYATERM_WEB_PASSWORD_FILE=/run/secrets/login",
      "-e",
      "NYATERM_WEB_ENCRYPTION_KEY_FILE=/run/secrets/encryption",
      ...mount("login", "login"),
      ...mount("encryption", "encryption"),
      "--mount",
      `type=volume,src=${webVolume},dst=/data`,
      "--read-only",
      "--tmpfs",
      "/tmp",
      "--cap-drop",
      "ALL",
      "--security-opt",
      "no-new-privileges:true",
      image,
    ]);
    try {
      const deadline = Date.now() + 60_000;
      for (;;) {
        try {
          if ((await fetch(url, { signal: AbortSignal.timeout(2000) })).ok)
            break;
        } catch {}
        if (Date.now() >= deadline) throw new Error("Web readiness timed out");
        await delay(500);
      }
      const result = spawnSync(
        process.execPath,
        [
          "node_modules/@playwright/test/cli.js",
          "test",
          "--config",
          "e2e/playwright.config.mjs",
        ],
        {
          stdio: "inherit",
          env: {
            ...process.env,
            NYATERM_E2E_URL: url,
            NYATERM_E2E_PASSWORD: password,
            NYATERM_E2E_SSH_PASSWORD: sshPassword,
            NYATERM_E2E_ARTIFACTS: join(output, "results"),
          },
        },
      );
      if (result.error || result.status !== 0)
        throw new Error(`Deployment acceptance failed for ${basePath}`);
    } catch (error) {
      failures.push(error);
    } finally {
      for (const container of [ssh, web])
        writeFileSync(
          join(output, `${container.endsWith("ssh") ? "ssh" : "web"}.log`),
          docker(["logs", container], true, true),
        );
      docker(["rm", "-f", web, ssh], true, true);
    }
  }
  if (failures.length)
    throw new AggregateError(failures, "Deployment acceptance failed");
} finally {
  for (const container of containers)
    docker(["rm", "-f", container], true, true);
  for (const volume of volumes) docker(["volume", "rm", volume], true, true);
  if (networkCreated) docker(["network", "rm", id], true, true);
  for (const image of images) docker(["image", "rm", image], true, true);
  const secretRelative = relative(resolve(tmpdir()), resolve(secrets));
  if (
    secretRelative.startsWith("..") ||
    !basename(secrets).startsWith(`${id}-`)
  )
    throw new Error("Refusing to remove an unexpected secrets directory");
  rmSync(secrets, { recursive: true, force: true });
}
