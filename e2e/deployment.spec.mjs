import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";

const en = JSON.parse(
  readFileSync(new URL("../src/i18n/locales/en.json", import.meta.url), "utf8"),
);
const t = (key) => key.split(".").reduce((value, part) => value[part], en);

test("single administrator: login, SSH, refresh, editor, conflicts, download, child settings, logout", async ({
  page,
}, testInfo) => {
  if (
    !process.env.NYATERM_E2E_PASSWORD ||
    !process.env.NYATERM_E2E_SSH_PASSWORD
  )
    throw new Error(
      "Run pnpm test:web:e2e to provision disposable Docker credentials.",
    );
  const base = process.env.NYATERM_E2E_URL;
  const errors = [],
    output = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("websocket", (socket) => {
    if (socket.url().endsWith("/terminal"))
      socket.on("framereceived", ({ payload }) => {
        if (Buffer.isBuffer(payload)) output.push(payload);
      });
  });
  const terminalOutput = () => Buffer.concat(output).toString("utf8");
  try {
    await page.goto(base);
    await page
      .getByLabel(t("web.password"))
      .fill(process.env.NYATERM_E2E_PASSWORD);
    await page
      .getByRole("button", { name: t("web.signIn"), exact: true })
      .click();
    await expect(page.getByLabel(t("web.password"))).toHaveCount(0);
    const auth = await (
      await page.request.get(new URL("api/auth/session", base).href)
    ).json();
    const headers = {
      "X-Nyaterm-Csrf": auth.csrf,
      "X-Nyaterm-Request": "1",
      Origin: new URL(base).origin,
    };
    const command = async (name, args = {}) => {
      const response = await page.request.post(
        new URL(`api/commands/${name}`, base).href,
        { headers, data: args },
      );
      expect(response.ok(), `${name}: ${await response.text()}`).toBe(true);
      return response.json();
    };
    const settings = await command("get_app_settings");
    settings.general.startup_restore = true;
    settings.ui.language = "en";
    settings.ui.file_explorer_view_mode = "list";
    settings.ui.active_left_panel = "fileExplorer";
    settings.ui.active_right_panel = "savedConnections";
    settings.ui.left_open_panels = ["fileExplorer"];
    settings.ui.right_open_panels = ["savedConnections"];
    settings.transfer.editor_type = "external"; // Web must override this without persisting a change.
    settings.transfer.internal_editor_display = "workspace";
    settings.transfer.duplicate_strategy = "ask";
    await command("save_app_settings", { settings });
    await command("save_connection", {
      connection: {
        id: "",
        name: "E2E OpenSSH",
        type: "ssh",
        host: "ssh-fixture",
        port: 22,
        username: "test",
        auth: {
          mode: "password",
          password: process.env.NYATERM_E2E_SSH_PASSWORD,
        },
      },
    });
    await page.reload();
    await page.getByText("E2E OpenSSH", { exact: true }).first().dblclick();
    await expect(
      page.getByText(t("settings.hostKeyVerifyTitle"), { exact: true }),
    ).toBeVisible();
    await page
      .getByRole("button", {
        name: t("settings.hostKeyVerifyAccept"),
        exact: true,
      })
      .click();
    await expect
      .poll(
        async () =>
          (await command("get_sessions")).filter((session) => session.connected)
            .length,
      )
      .toBe(1);
    const sessionId = (await command("get_sessions"))[0].id;
    expect(sessionId).toBeTruthy();
    async function typeCommand(marker) {
      const terminal = page.locator(".xterm-helper-textarea").first();
      await terminal.focus();
      await terminal.pressSequentially(`printf 'WEB_%s\\n' '${marker}'`, {
        delay: 5,
      });
      await terminal.press("Enter");
      await expect.poll(terminalOutput).toContain(`WEB_${marker}`);
    }
    await typeCommand("READY");
    await expect
      .poll(async () => (await command("get_app_settings")).ui.open_tabs.length)
      .toBeGreaterThan(0);
    await page.reload();
    await expect(page.locator(".xterm-helper-textarea").first()).toBeAttached();
    await typeCommand("REFRESHED");
    expect((await command("get_sessions"))[0].id).toBe(sessionId);

    const readme = page.getByText("readme.txt", { exact: true }).first();
    await readme.dblclick();
    const editor = page.locator(".cm-content").first();
    await expect(editor).toContainText("fixture text");
    await editor.fill("edited through Chromium\n");
    await page
      .getByRole("button", { name: t("fileEditor.save"), exact: true })
      .click();
    await expect
      .poll(
        async () =>
          (
            await command("open_remote_file_text", {
              sessionId,
              path: "/home/test/readme.txt",
            })
          ).file.content,
      )
      .toBe("edited through Chromium\n");
    expect((await command("get_app_settings")).transfer.editor_type).toBe(
      "external",
    );

    const binaryDownload = page.waitForEvent("download");
    await page.getByText("binary.bin", { exact: true }).first().dblclick();
    const binary = await binaryDownload;
    expect(readFileSync(await binary.path())).toEqual(
      Buffer.from([0, 1, 2, 3]),
    );
    await expect(
      page.getByText(t("fileExplorer.webUnsupportedDownload")),
    ).toBeVisible();

    async function upload(content) {
      await page
        .getByRole("button", { name: t("fileExplorer.upload"), exact: true })
        .click();
      const chooser = page.waitForEvent("filechooser");
      await page
        .getByRole("menuitem", { name: t("fileExplorer.upload"), exact: true })
        .click();
      await (
        await chooser
      ).setFiles({
        name: "readme.txt",
        mimeType: "text/plain",
        buffer: Buffer.from(content),
      });
    }
    for (const action of ["duplicateSkip", "duplicateOverwrite"]) {
      const response = page.waitForResponse((response) =>
        response.url().includes(`/sessions/${sessionId}/upload?`),
      );
      await upload("browser upload\n");
      await page
        .getByRole("button", { name: t(`fileTransfer.${action}`), exact: true })
        .click();
      const result = await (await response).json();
      expect(result.status).toBe(
        action === "duplicateSkip" ? "skipped" : "completed",
      );
      expect(result.path).toBe("/home/test/readme.txt");
      const saved = await command("open_remote_file_text", {
        sessionId,
        path: result.path,
      });
      expect(saved.file.content).toBe(
        action === "duplicateSkip"
          ? "edited through Chromium\n"
          : "browser upload\n",
      );
    }
    const current = await command("get_app_settings");
    current.transfer.duplicate_strategy = "rename";
    await command("save_app_settings", { settings: current });
    const renamedResponse = page.waitForResponse((response) =>
      response.url().includes(`/sessions/${sessionId}/upload?`),
    );
    await upload("renamed browser upload\n");
    const renamed = await (await renamedResponse).json();
    expect(renamed.path).toMatch(/^\/home\/test\/readme\.txt\.[0-9a-f-]{36}$/);
    expect(
      (
        await command("open_remote_file_text", {
          sessionId,
          path: renamed.path,
        })
      ).file.content,
    ).toBe("renamed browser upload\n");
    const downloaded = page.waitForEvent("download");
    const readmeRow = page.getByRole("listitem").filter({
      has: page.getByText("readme.txt", { exact: true }),
    });
    await expect(readmeRow).toHaveCount(1);
    await readmeRow.click({ button: "right" });
    await page
      .getByRole("menuitem", {
        name: t("fileExplorer.cmDownload"),
        exact: true,
      })
      .click();
    expect(readFileSync(await (await downloaded).path()).toString()).toBe(
      "browser upload\n",
    );

    await page
      .getByRole("button", { name: t("settings.title"), exact: true })
      .click();
    const child = page.frameLocator('iframe[src*="window=settings"]');
    await child
      .getByRole("button", { name: t("settings.transfer"), exact: true })
      .click();
    await expect(child.getByRole("combobox")).toHaveCount(2);
    await expect(
      child.getByText(t("settings.downloadPath"), { exact: true }),
    ).toHaveCount(0);
    await child.getByRole("combobox").nth(1).click();
    await child
      .getByRole("option", {
        name: t("settings.internalEditorDisplayWindow"),
        exact: true,
      })
      .click();
    await child
      .getByRole("button", { name: t("common.confirm"), exact: true })
      .click();
    await expect(page.locator('iframe[src*="window=settings"]')).toHaveCount(0);
    await page
      .getByText(renamed.path.split("/").pop(), { exact: true })
      .first()
      .dblclick();
    const fileChild = page.frameLocator('iframe[src*="window=file-editor"]');
    await expect(fileChild.locator(".cm-content")).toContainText(
      "renamed browser upload",
    );
    await fileChild
      .getByRole("button", { name: t("common.close"), exact: true })
      .last()
      .click();
    await expect(page.locator('iframe[src*="window=file-editor"]')).toHaveCount(
      0,
    );

    await page.getByText(t("menu.file"), { exact: true }).first().click();
    await page.getByText(t("web.signOut"), { exact: true }).click();
    const quit = page.getByRole("button", {
      name: t("dialog.confirmCloseAction"),
      exact: true,
    });
    if (await quit.isVisible()) await quit.click();
    await expect(page.getByLabel(t("web.password"))).toBeVisible();
    expect(
      (await page.request.get(new URL("api/auth/session", base).href)).status(),
    ).toBe(401);
    expect(errors).toEqual([]);
  } finally {
    await testInfo.attach("browser-errors", {
      body: errors.join("\n"),
      contentType: "text/plain",
    });
  }
});
