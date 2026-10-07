import { type Dispatch, type SetStateAction, useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { AppContextType } from "@/context/AppContext";
import {
  closeStaleCreatedSession,
  focusTerminalSession,
  getConnectionSessionType,
  getRemoteDesktopPaneDisplay,
  isSessionCreationCancelled,
  launchSavedRdpWithSystemClient,
} from "@/lib/appSessionFactory";
import { getErrorMessage, shouldPromptConnectionEditOnFailure } from "@/lib/errors";
import { invoke } from "@/lib/invoke";
import { insertTabIntoLeaf, type TerminalWindowNode } from "@/lib/tabWindows";
import { openNewSession } from "@/lib/windowManager";
import { findSessionPaneById, getActivePane } from "@/lib/workspaceTabs";
import type { GeneralSettings, SavedConnection } from "@/types/global";

export interface SessionConnectAfterEditPayload {
  connectionId: string;
  targetLeafId?: string;
  anchorTabId?: string | null;
  sourceTabId?: string;
  sourcePaneId?: string;
  targetWindowLabel?: string | null;
}

interface ConnectAfterEditOptions
  extends Pick<
    AppContextType,
    | "tabs"
    | "addPendingTab"
    | "hasPane"
    | "hasTab"
    | "markPaneConnecting"
    | "markPaneConnectionFailed"
    | "markTabConnectionFailed"
    | "recordRecentConnection"
    | "setActivePane"
    | "setActiveTabId"
    | "updatePaneSession"
    | "updateTabSession"
  > {
  setTerminalWindows: Dispatch<SetStateAction<TerminalWindowNode | null>>;
  updateAutoIconForSessionStart: (connectionId: string, sessionId: string) => void;
  rdpClientMode: GeneralSettings["rdp_client_mode"];
}

export function useConnectAfterEdit({
  tabs,
  addPendingTab,
  hasPane,
  hasTab,
  markPaneConnecting,
  markPaneConnectionFailed,
  markTabConnectionFailed,
  recordRecentConnection,
  setActivePane,
  setActiveTabId,
  updatePaneSession,
  updateTabSession,
  setTerminalWindows,
  updateAutoIconForSessionStart,
  rdpClientMode,
}: ConnectAfterEditOptions) {
  const { t } = useTranslation();
  const tabsRef = useRef(tabs);
  useEffect(() => {
    tabsRef.current = tabs;
  }, [tabs]);

  return useCallback(
    async (payload: SessionConnectAfterEditPayload) => {
      const { connectionId, targetLeafId, anchorTabId, sourceTabId, sourcePaneId } = payload;
      try {
        const conns = await invoke<SavedConnection[]>("get_saved_connections");
        const conn = conns.find((c) => c.id === connectionId);
        const connName = conn?.name ?? connectionId;
        if (conn) {
          try {
            if (await launchSavedRdpWithSystemClient(conn, rdpClientMode)) {
              recordRecentConnection(connectionId);
              return;
            }
          } catch (error) {
            const errorMessage = getErrorMessage(error);
            toast.error(
              t("savedConnections.connectionFailed", { error: errorMessage }),
            );
            return;
          }
        }
        const sessionType = getConnectionSessionType(conn);
        const sourceTab = sourceTabId
          ? (tabsRef.current.find((item) => item.id === sourceTabId) ?? null)
          : null;
        const sourcePane =
          sourceTab &&
          ((sourcePaneId ? findSessionPaneById(sourceTab.root, sourcePaneId) : null) ??
            getActivePane(sourceTab));
        let tabId: string;
        let paneId: string | undefined;
        let createRequestId: string | null = null;

        if (sourceTab && sourcePane) {
          tabId = sourceTab.id;
          paneId = sourcePane.id;
          setActiveTabId(tabId);
          setActivePane(tabId, paneId);
          createRequestId = markPaneConnecting(tabId, paneId, {
            name: connName,
            type: sessionType,
            connectionId,
            display: getRemoteDesktopPaneDisplay(conn),
          });
        } else {
          const pending = addPendingTab(
            connName,
            sessionType,
            connectionId,
            undefined,
            anchorTabId ? { afterTabId: anchorTabId } : undefined,
            { display: getRemoteDesktopPaneDisplay(conn) },
          );
          tabId = pending.tabId;
          paneId = pending.paneId;
          createRequestId = pending.createRequestId;
          if (targetLeafId) {
            setTerminalWindows((current) =>
              current
                ? insertTabIntoLeaf(current, targetLeafId, tabId, {
                    afterTabId: anchorTabId,
                    activeTabId: tabId,
                  })
                : current,
            );
          }
        }

        try {
          let sessionId: string;
          switch (conn?.type) {
            case "local_terminal":
              sessionId = await invoke<string>("create_local_session", {
                connectionId,
                createRequestId,
                recordingScopeId: paneId,
              });
              break;
            case "telnet":
              sessionId = await invoke<string>("create_telnet_session", {
                connectionId,
                createRequestId,
                recordingScopeId: paneId,
              });
              break;
            case "serial":
              sessionId = await invoke<string>("create_serial_session", {
                connectionId,
                createRequestId,
                recordingScopeId: paneId,
              });
              break;
            case "rdp":
              sessionId = await invoke<string>("create_rdp_session", {
                connectionId,
                createRequestId,
              });
              break;
            default:
              sessionId = await invoke<string>("create_ssh_session", {
                connectionId,
                createRequestId,
                recordingScopeId: paneId,
              });
              break;
          }
          if (paneId ? !hasPane(tabId, paneId) : !hasTab(tabId)) {
            await closeStaleCreatedSession(sessionId);
            return;
          }
          if (paneId) {
            updatePaneSession(tabId, paneId, sessionId);
          } else {
            updateTabSession(tabId, sessionId);
          }
          focusTerminalSession(sessionId);
          recordRecentConnection(connectionId);
          updateAutoIconForSessionStart(connectionId, sessionId);
        } catch (error) {
          if (
            isSessionCreationCancelled(error) ||
            (paneId ? !hasPane(tabId, paneId) : !hasTab(tabId))
          ) {
            return;
          }
          const errorMessage = getErrorMessage(error);
          if (paneId) {
            markPaneConnectionFailed(tabId, paneId, errorMessage);
          } else {
            markTabConnectionFailed(tabId, errorMessage);
          }
          if (shouldPromptConnectionEditOnFailure(conn, errorMessage)) {
            openNewSession(connectionId, true, {
              sourceTabId: tabId,
              sourcePaneId: paneId,
            });
          }
        }
      } catch {
        /* ignore */
      }
    },
    [
      addPendingTab,
      hasPane,
      hasTab,
      markPaneConnecting,
      markPaneConnectionFailed,
      markTabConnectionFailed,
      recordRecentConnection,
      rdpClientMode,
      setActivePane,
      setActiveTabId,
      updatePaneSession,
      updateTabSession,
      setTerminalWindows,
      updateAutoIconForSessionStart,
      t,
    ],
  );
}
