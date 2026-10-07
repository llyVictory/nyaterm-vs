import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_AI_SETTINGS } from "@/lib/aiSettings";
import type {
  AIMessage,
  AISession,
  AISessionScope,
  Group,
  SavedConnection,
  Tab,
  TerminalSessionPane,
} from "@/types/global";
import AIAssistantPanel from "./AIAssistantPanel";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

let appState: {
  appSettings: {
    ai: typeof DEFAULT_AI_SETTINGS;
    ui: { language: string };
  };
  updateAppSettings: ReturnType<typeof vi.fn>;
  tabs: Tab[];
  savedConnections: SavedConnection[];
  savedGroups: Group[];
};

vi.mock("@/context/AppContext", () => ({
  useApp: () => appState,
}));

vi.mock("@/context/ThemeContext", () => ({
  useTheme: () => ({ theme: { colors: {} } }),
}));

vi.mock("@/lib/invoke", () => ({ invoke: invokeMock }));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock("react-i18next", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-i18next")>();
  return {
    ...actual,
    useTranslation: () => ({ t: (key: string) => key }),
  };
});

vi.mock("@/components/dialog/ai/AIAssistantDialogs", () => ({
  AIAssistantDialogs: () => null,
}));

vi.mock("./ModelCombobox", () => ({
  ModelCombobox: () => null,
}));

vi.mock("./utils", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./utils")>();
  return { ...actual, buildPrismThemeFromColors: () => ({}) };
});

describe("AIAssistantPanel history scope ownership", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    appState = {
      appSettings: {
        ai: { ...DEFAULT_AI_SETTINGS },
        ui: { language: "zh-CN" },
      },
      updateAppSettings: vi.fn(),
      tabs: [],
      savedConnections: [],
      savedGroups: [],
    };
  });

  it("passes only the selected connection metadata and its effective runtime profile to chat", async () => {
    const pane = terminalPane("selected-session");
    appState.tabs = [tabWithPane(pane)];
    appState.savedGroups = [{ id: "prod", name: "Production", sort_order: 0 }];
    appState.savedConnections = [
      {
        id: "connection-1",
        type: "ssh",
        name: "API",
        host: "api.example",
        port: 2222,
        username: "ops",
        description: "Selected API server",
        tags: ["prod"],
        group_id: "prod",
        auth: { mode: "password", password: "do-not-send" },
      },
      {
        id: "other",
        type: "ssh",
        name: "Other",
        description: "unselected connection",
      },
    ];
    appState.appSettings.ai = {
      ...DEFAULT_AI_SETTINGS,
      enabled: true,
      default_model_id: "test-model",
      models: [
        {
          id: "test-model",
          name: "Test model",
          provider_kind: "openai",
          enabled: true,
          source: "manual",
        },
      ],
    };
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "get_ai_sessions":
          return Promise.resolve([]);
        case "list_sessions":
          return Promise.resolve([
            {
              id: pane.sessionId,
              session_type: "SSH",
              ai_execution_profile: "posix",
            },
          ]);
        case "get_terminal_cwd":
          return Promise.resolve("/srv/api");
        case "start_ai_chat_stream":
          return Promise.resolve({ sessionId: "new-chat" });
        case "append_ai_audit":
          return Promise.resolve(null);
        default:
          return Promise.reject(new Error(`Unexpected command: ${command}`));
      }
    });
    render(
      <AIAssistantPanel
        activePane={pane}
        intent={{
          id: "metadata-intent",
          action: "generate_command",
          userInput: "inspect server",
        }}
      />,
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "start_ai_chat_stream",
        expect.anything(),
      ),
    );
    const request = invokeMock.mock.calls.find(
      ([command]) => command === "start_ai_chat_stream",
    )?.[1].request;
    expect(request.context).toMatchObject({
      connectionName: "API",
      host: "api.example",
      port: 2222,
      description: "Selected API server",
      tags: ["prod"],
      groupPath: ["Production"],
      executionProfile: "posix",
      cwd: "/srv/api",
    });
    expect(request.targetContexts).toHaveLength(1);
    expect(request.targetContexts[0].context).toEqual(request.context);
    expect(JSON.stringify(request)).not.toContain("do-not-send");
    expect(JSON.stringify(request)).not.toContain("unselected connection");
    expect(
      invokeMock.mock.calls.filter(([command]) => command === "list_sessions"),
    ).toHaveLength(1);
  });

  it("allows a history session to move after its terminal reconnects with a new session id", async () => {
    const oldPane = terminalPane("old-session");
    const newPane = terminalPane("new-session");
    let historySession = aiSession("ai-session", oldPane.sessionId);
    let messageLoadCount = 0;

    invokeMock.mockImplementation(
      (command: string, args?: Record<string, unknown>) => {
        switch (command) {
          case "get_ai_sessions":
            return Promise.resolve([historySession]);
          case "get_ai_messages": {
            messageLoadCount += 1;
            return Promise.resolve([
              aiMessage(
                historySession.id,
                messageLoadCount === 1 ? "before reconnect" : "after reconnect",
              ),
            ]);
          }
          case "rebind_ai_session":
            historySession = {
              ...historySession,
              scope: args?.ownerScope as AISessionScope,
            };
            return Promise.resolve(historySession);
          default:
            return Promise.reject(new Error(`Unexpected command: ${command}`));
        }
      },
    );

    appState.tabs = [tabWithPane(oldPane)];
    const view = render(
      <AIAssistantPanel activePane={oldPane} intent={null} />,
    );

    openHistory(view.container);
    fireEvent.click(await historySessionButton(historySession.title));
    await screen.findByText("before reconnect");
    expect(invokeMock).not.toHaveBeenCalledWith(
      "rebind_ai_session",
      expect.anything(),
    );

    appState.tabs = [tabWithPane(newPane)];
    view.rerender(<AIAssistantPanel activePane={newPane} intent={null} />);

    openHistory(view.container);
    const movableSessionButton = await historySessionButton(
      historySession.title,
    );
    expect(movableSessionButton.disabled).toBe(false);
    expect(screen.queryByText("ai.historyInUse")).toBeNull();
    expect(screen.getByText("ai.historyMoveToCurrent")).not.toBeNull();

    fireEvent.click(movableSessionButton);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("rebind_ai_session", {
        sessionId: historySession.id,
        ownerScope: {
          type: "terminal",
          targetId: newPane.sessionId,
          connectionIds: [newPane.connectionId],
          label: newPane.name,
        },
      });
    });
    await screen.findByText("after reconnect");
    expect(messageLoadCount).toBe(2);
  });

  it("keeps history locked while the owning terminal pane still exists", async () => {
    const owningPane = terminalPane("owning-session", {
      connectError: "connection lost",
    });
    const currentPane = terminalPane("current-session", {
      id: "current-pane",
      connectionId: "current-connection",
      name: "Current terminal",
    });
    const historySession = aiSession("shared-ai-session", owningPane.sessionId);

    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "get_ai_sessions":
          return Promise.resolve([historySession]);
        case "get_ai_messages":
          return Promise.resolve([]);
        default:
          return Promise.reject(new Error(`Unexpected command: ${command}`));
      }
    });

    appState.tabs = [tabWithPane(owningPane, 0), tabWithPane(currentPane, 1)];
    const view = render(
      <AIAssistantPanel activePane={owningPane} intent={null} />,
    );

    openHistory(view.container);
    fireEvent.click(await historySessionButton(historySession.title));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("get_ai_messages", {
        sessionId: historySession.id,
      });
    });

    view.rerender(<AIAssistantPanel activePane={currentPane} intent={null} />);
    openHistory(view.container);

    const lockedSessionButton = await historySessionButton(
      historySession.title,
    );
    expect(lockedSessionButton.disabled).toBe(true);
    expect(screen.getByText("ai.historyInUse")).not.toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "rebind_ai_session",
      expect.anything(),
    );
  });

  it("keeps history locked while its AI stream is still running after the terminal closes", async () => {
    const owningPane = terminalPane("stream-owner");
    const currentPane = terminalPane("current-session", {
      id: "current-pane",
      connectionId: "current-connection",
      name: "Current terminal",
    });
    const historySession = aiSession("stream-ai-session", owningPane.sessionId);

    appState.appSettings.ai = {
      ...DEFAULT_AI_SETTINGS,
      enabled: true,
      default_model_id: "test-model",
      models: [
        {
          id: "test-model",
          name: "Test model",
          provider_kind: "openai",
          enabled: true,
          source: "manual",
        },
      ],
    };

    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "get_ai_sessions":
          return Promise.resolve([historySession]);
        case "start_ai_chat_stream":
          return Promise.resolve({ sessionId: historySession.id });
        case "append_ai_audit":
          return Promise.resolve(null);
        default:
          return Promise.reject(new Error(`Unexpected command: ${command}`));
      }
    });

    appState.tabs = [tabWithPane(owningPane)];
    const intent = {
      id: "stream-intent",
      action: "generate_command" as const,
      userInput: "keep streaming",
    };
    const view = render(
      <AIAssistantPanel activePane={owningPane} intent={intent} />,
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "start_ai_chat_stream",
        expect.objectContaining({
          request: expect.objectContaining({ sessionId: null }),
        }),
      );
    });

    appState.tabs = [tabWithPane(currentPane)];
    view.rerender(
      <AIAssistantPanel activePane={currentPane} intent={intent} />,
    );
    openHistory(view.container);

    const lockedSessionButton = await historySessionButton(
      historySession.title,
    );
    expect(lockedSessionButton.disabled).toBe(true);
    expect(screen.getByText("ai.historyInUse")).not.toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "rebind_ai_session",
      expect.anything(),
    );
  });
});

function openHistory(container: HTMLElement) {
  const button = container.querySelector<HTMLButtonElement>(
    'button[aria-expanded]:not([aria-label]):not([role="combobox"])',
  );
  if (!button) throw new Error("History button not found");
  fireEvent.click(button);
}

async function historySessionButton(title: string) {
  const titleElement = await screen.findByText(title);
  const button = titleElement.closest("button");
  if (!(button instanceof HTMLButtonElement))
    throw new Error("History session button not found");
  return button;
}

function terminalPane(
  sessionId: string,
  overrides: Partial<TerminalSessionPane> = {},
): TerminalSessionPane {
  return {
    id: "terminal-pane",
    kind: "leaf",
    paneKind: "terminal",
    sessionId,
    name: "SSH terminal",
    type: "SSH",
    connectionId: "connection-1",
    ...overrides,
  };
}

function tabWithPane(pane: TerminalSessionPane, persistOrder = 0): Tab {
  return {
    id: `tab-${pane.id}`,
    persistOrder,
    activePaneId: pane.id,
    root: pane,
  };
}

function aiSession(id: string, targetId: string): AISession {
  return {
    id,
    title: "Reconnect chat",
    createdAt: "2026-09-07T10:00:00Z",
    updatedAt: "2026-09-07T10:05:00Z",
    connectionId: "connection-1",
    scope: {
      type: "terminal",
      targetId,
      connectionIds: ["connection-1"],
      label: "SSH terminal",
    },
  };
}

function aiMessage(sessionId: string, content: string): AIMessage {
  return {
    id: `message-${content}`,
    sessionId,
    role: "user",
    content,
    createdAt: "2026-09-07T10:01:00Z",
  };
}
