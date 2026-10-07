import { useEffect } from "react";
import { runtime } from "@/lib/backend/runtime";
import { invoke } from "@/lib/invoke";

export function useMcpActiveSession(activeSessionId: string | null) {
  useEffect(() => {
    if (runtime !== "desktop") return;
    void invoke("report_mcp_active_session", {
      sessionId: activeSessionId,
    }).catch(() => {});
  }, [activeSessionId]);
}
