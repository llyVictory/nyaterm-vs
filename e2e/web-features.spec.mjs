import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";

const locale = (language) =>
  JSON.parse(
    readFileSync(
      new URL(`../src/i18n/locales/${language}.json`, import.meta.url),
      "utf8",
    ),
  );
const en = locale("en");
const t = (key, dictionary = en) =>
  key.split(".").reduce((value, part) => value[part], dictionary);

test("login layouts in four languages, light/dark themes and narrow screens", async ({
  browser,
}, testInfo) => {
  for (const language of ["en", "zh-CN", "zh-TW", "ko"]) {
    for (const theme of ["github-light", "github-dark"]) {
      const context = await browser.newContext({
        viewport: { width: 360, height: 760 },
        locale: language,
      });
      await context.addInitScript(
        ({ language, theme }) => {
          localStorage.setItem("nyaterm-web-language", language);
          localStorage.setItem("df-theme-id", theme);
        },
        { language, theme },
      );
      const page = await context.newPage();
      await page.goto(process.env.NYATERM_E2E_URL);
      const password = page.getByLabel(t("web.password", locale(language)), {
        exact: true,
      });
      await expect(password).toBeEnabled();
      await expect(password).toBeFocused();
      await expect(
        page.getByRole("heading", { name: t("web.welcome", locale(language)) }),
      ).toBeVisible();
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth,
        ),
      ).toBe(true);
      await expect(
        page.locator('img[src$="icons/app/nyaterm.svg"]'),
      ).toBeVisible();
      const image = await page.screenshot({
        path: testInfo.outputPath(`login-${language}-${theme}-360px.png`),
      });
      await testInfo.attach(`login-${language}-${theme}-360px`, {
        body: image,
        contentType: "image/png",
      });
      await context.close();
    }
  }
});

test("password errors, keyboard login, SSE retry, portable backup and diagnostic downloads", async ({
  page,
}) => {
  if (!process.env.NYATERM_E2E_PASSWORD)
    throw new Error("Run scripts/web-local-e2e.mjs or pnpm test:web:e2e");
  const base = process.env.NYATERM_E2E_URL;
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await page.goto(base);
  const password = page.getByLabel(t("web.password"), { exact: true });
  await expect(password).toBeEnabled();
  await password.fill("incorrect-password");
  await password.press("Enter");
  await expect(page.getByRole("alert")).toHaveText(t("web.invalidPassword"));
  await page.getByRole("button", { name: t("web.showPassword") }).click();
  await expect(password).toHaveAttribute("type", "text");
  await page.getByRole("button", { name: t("web.hidePassword") }).click();

  // Faults are injected only at the browser transport; login and restore use the real server.
  await page.route("**/api/auth/login", (route) =>
    route.fulfill({
      status: 429,
      contentType: "application/json",
      body: JSON.stringify({ error: "Limited" }),
    }),
  );
  await password.press("Enter");
  await expect(page.getByRole("alert")).toHaveText(t("web.signInRateLimited"));
  await page.unroute("**/api/auth/login");
  await page.route("**/api/auth/login", (route) =>
    route.abort("internetdisconnected"),
  );
  await password.press("Enter");
  await expect(page.getByRole("alert")).toHaveText(t("web.serverUnavailable"));
  await page.unroute("**/api/auth/login");
  await page.route("**/api/events", (route) =>
    route.abort("internetdisconnected"),
  );
  let loginCount = 0;
  page.on("request", (request) => {
    if (request.url().endsWith("/api/auth/login")) loginCount++;
  });
  await password.fill(process.env.NYATERM_E2E_PASSWORD);
  await password.press("Enter");
  await expect(page.getByRole("alert")).toHaveText(
    t("web.eventConnectionFailed"),
  );
  await page.unroute("**/api/events");
  await page.getByRole("button", { name: t("web.retryConnection") }).click();
  await expect(password).toHaveCount(0);
  expect(loginCount).toBe(1);
  await page.reload();
  await expect(
    page.getByText(t("menu.file"), { exact: true }).first(),
  ).toBeVisible();
  expect(loginCount).toBe(1);

  const auth = await (
    await page.request.get(new URL("api/auth/session", base).href)
  ).json();
  const headers = {
    Origin: new URL(base).origin,
    "X-Nyaterm-Request": "1",
    "X-Nyaterm-Csrf": auth.csrf,
  };
  const command = async (name, args = {}) => {
    const id = randomUUID();
    const response = await page.request.post(
      new URL(`api/commands/${name}`, base).href,
      { headers: { ...headers, "X-Nyaterm-Request-Id": id }, data: args },
    );
    expect(response.headers()["x-nyaterm-request-id"]).toBe(id);
    expect(response.ok(), `${name}: ${await response.text()}`).toBe(true);
    return response.json();
  };
  await command("save_password", {
    entry: {
      id: "web-e2e-password",
      name: "Web backup fixture",
      password: "portable-secret",
    },
  });
  const menu = async (key) => {
    await page.getByText(t("menu.file"), { exact: true }).first().click();
    await page.getByText(t(key), { exact: true }).click();
  };
  await menu("settings.exportConfig");
  await page
    .getByLabel(t("web.backupPassword"), { exact: true })
    .fill("web-e2e-backup-password");
  await page
    .getByLabel(t("web.backupPasswordConfirm"), { exact: true })
    .fill("different");
  await expect(
    page.getByRole("button", { name: t("web.backupExportTitle"), exact: true }),
  ).toBeDisabled();
  await page
    .getByLabel(t("web.backupPasswordConfirm"), { exact: true })
    .fill("web-e2e-backup-password");
  const download = page.waitForEvent("download");
  await page
    .getByRole("button", { name: t("web.backupExportTitle"), exact: true })
    .click();
  const backup = await download;
  expect(backup.suggestedFilename()).toMatch(/\.nya$/);
  const backupBytes = readFileSync(await backup.path());
  expect(backupBytes.includes(Buffer.from("portable-secret"))).toBe(false);
  await command("delete_password", { id: "web-e2e-password" });
  await menu("settings.importConfig");
  await page.getByRole("button", { name: /NyaTerm \(\.nya\)/ }).click();
  const chooser = page.waitForEvent("filechooser");
  await page
    .getByRole("button", {
      name: t("savedConnections.restoreBackupConfirmAction"),
      exact: true,
    })
    .click();
  await (
    await chooser
  ).setFiles({
    name: "fixture.nya",
    mimeType: "application/octet-stream",
    buffer: backupBytes,
  });
  await page
    .getByLabel(t("web.backupPassword"), { exact: true })
    .fill("wrong-password");
  await page
    .getByLabel(t("web.backupOverwriteConfirm"), { exact: true })
    .check();
  await page
    .getByRole("button", { name: t("web.backupImportTitle"), exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText(
    t("web.backupImportFailed"),
  );
  await page
    .getByLabel(t("web.backupPassword"), { exact: true })
    .fill("web-e2e-backup-password");
  await page
    .getByRole("button", { name: t("web.backupImportTitle"), exact: true })
    .click();
  await expect(
    page.getByLabel(t("web.backupPassword"), { exact: true }),
  ).toHaveCount(0);
  expect(
    await command("get_saved_password_value", { id: "web-e2e-password" }),
  ).toBe("portable-secret");

  await command("save_quick_commands", {
    config: {
      commands: [
        {
          id: "web-quick-fixture",
          label: "Web quick fixture",
          command: "printf fixture",
        },
      ],
      categories: [],
    },
  });
  const settings = await command("get_app_settings");
  settings.ui.active_right_panel = "quickCommands";
  settings.ui.right_open_panels = ["quickCommands"];
  await command("save_app_settings", { settings });
  await page.reload();
  await expect(
    page.getByText("Web quick fixture", { exact: true }),
  ).toBeVisible();
  const quickDownload = page.waitForEvent("download");
  await page
    .getByRole("button", { name: t("quickCommands.export"), exact: true })
    .click();
  const quickBytes = readFileSync(await (await quickDownload).path());
  expect(
    JSON.parse(quickBytes.toString()).commands.some(
      (entry) => entry.id === "web-quick-fixture",
    ),
  ).toBe(true);
  await command("save_quick_commands", {
    config: { commands: [], categories: [] },
  });
  await page
    .getByRole("button", { name: t("quickCommands.import"), exact: true })
    .click();
  const quickChooser = page.waitForEvent("filechooser");
  await page
    .getByRole("button", {
      name: new RegExp(t("quickCommands.importNyaTermJson")),
    })
    .click();
  await (
    await quickChooser
  ).setFiles({
    name: "quick.json",
    mimeType: "application/json",
    buffer: quickBytes,
  });
  await expect
    .poll(async () =>
      (await command("get_quick_commands")).commands.some(
        (entry) => entry.id === "web-quick-fixture",
      ),
    )
    .toBe(true);

  await page
    .getByRole("button", { name: t("settings.title"), exact: true })
    .click();
  const child = page.frameLocator('iframe[src*="window=settings"]');
  await child
    .getByRole("button", { name: t("settings.general"), exact: true })
    .first()
    .click();
  await expect(
    child.getByRole("button", { name: t("settings.openLogs"), exact: true }),
  ).toHaveCount(0);
  const diagnosticDownload = page.waitForEvent("download");
  await child
    .getByRole("button", { name: t("settings.exportDiagnostics"), exact: true })
    .click();
  expect(
    readFileSync(await (await diagnosticDownload).path())
      .subarray(0, 2)
      .toString(),
  ).toBe("PK");
  await child
    .getByRole("button", { name: t("settings.appearance"), exact: true })
    .first()
    .click();
  await child
    .getByRole("button", { name: t("settings.themeDesignerOpen"), exact: true })
    .click();
  await child
    .getByRole("button", {
      name: t("settings.themeDesignerCopyTheme"),
      exact: true,
    })
    .click();
  const themeDownload = page.waitForEvent("download");
  await child
    .getByRole("button", {
      name: t("settings.themeDesignerExport"),
      exact: true,
    })
    .click();
  const themeData = JSON.parse(
    readFileSync(await (await themeDownload).path()).toString(),
  );
  themeData.name = "Web theme fixture";
  const themeChooser = page.waitForEvent("filechooser");
  await child
    .getByRole("button", {
      name: t("settings.themeDesignerImport"),
      exact: true,
    })
    .click();
  await (
    await themeChooser
  ).setFiles({
    name: "theme.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(themeData)),
  });
  await expect
    .poll(async () =>
      (await command("get_app_settings")).appearance.custom_themes.some(
        (theme) => theme.name === "Web theme fixture",
      ),
    )
    .toBe(true);
  await child
    .getByRole("dialog")
    .getByRole("button", { name: t("common.close"), exact: true })
    .click();

  await child
    .getByRole("button", { name: t("common.confirm"), exact: true })
    .click();
  await expect(page.locator('iframe[src*="window=settings"]')).toHaveCount(0);
  await page.reload();
  await expect(
    page.getByText(t("menu.file"), { exact: true }).first(),
  ).toBeVisible();
  expect(errors).toEqual([]);
});
