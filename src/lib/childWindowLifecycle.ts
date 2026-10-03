import { emit } from "@/lib/backend/api";
import { getCurrentWindow } from "@/lib/backend/platform/window";
import {
  CHILD_WINDOW_LIFECYCLE_EVENT,
  CHILD_WINDOW_READY_TOKEN_PARAM,
  type ChildWindowCommandName,
  type ChildWindowLifecyclePayload,
  type ChildWindowLoadFailureStage,
} from "./childWindowProtocol";

// load-started 不阻塞 shell 渲染；后续信号复用该 Promise，保持单次加载内的顺序。
let loadStartedPromise: Promise<void> | undefined;

function lifecycleIdentity() {
  const token = new URLSearchParams(window.location.search).get(CHILD_WINDOW_READY_TOKEN_PARAM);
  return {
    label: getCurrentWindow().label,
    token: token ?? undefined,
  };
}

function emitChildWindowLifecycle(
  payload:
    | { phase: "load-started" }
    | { phase: "shell-ready" }
    | { phase: "command-ready"; command: ChildWindowCommandName }
    | { phase: "load-failed"; stage: ChildWindowLoadFailureStage },
) {
  return emit(CHILD_WINDOW_LIFECYCLE_EVENT, {
    ...lifecycleIdentity(),
    ...payload,
  } satisfies ChildWindowLifecyclePayload);
}

export function signalChildWindowLoadStarted() {
  loadStartedPromise ??= emitChildWindowLifecycle({ phase: "load-started" }).catch((error) => {
    loadStartedPromise = undefined;
    throw error;
  });
  return loadStartedPromise;
}

function signalChildWindowLifecycle(
  payload:
    | { phase: "shell-ready" }
    | { phase: "command-ready"; command: ChildWindowCommandName }
    | { phase: "load-failed"; stage: ChildWindowLoadFailureStage },
) {
  return signalChildWindowLoadStarted().then(() => emitChildWindowLifecycle(payload));
}

export function signalChildWindowCommandReady(command: ChildWindowCommandName) {
  return signalChildWindowLifecycle({ phase: "command-ready", command });
}

export function signalChildWindowLoadFailed(stage: ChildWindowLoadFailureStage) {
  return signalChildWindowLifecycle({ phase: "load-failed", stage });
}

/**
 * loading shell 形成稳定布局后再通知父窗口显示。隐藏 WebView 可能暂停 rAF，
 * 因此 fallback 只确认 shell 已挂载，不等待字体、provider 或业务页面。
 */
export function scheduleChildWindowShellReady() {
  let settled = false;
  let firstFrameId: number | undefined;
  let secondFrameId: number | undefined;
  let contentPollTimeoutId: number | undefined;
  let fallbackTimeoutId: number | undefined;

  const cleanup = () => {
    settled = true;
    if (firstFrameId !== undefined) window.cancelAnimationFrame(firstFrameId);
    if (secondFrameId !== undefined) window.cancelAnimationFrame(secondFrameId);
    if (contentPollTimeoutId !== undefined) window.clearTimeout(contentPollTimeoutId);
    if (fallbackTimeoutId !== undefined) window.clearTimeout(fallbackTimeoutId);
  };

  const hasMountedShell = () => {
    const root = document.getElementById("root");
    return Boolean(root?.firstElementChild);
  };

  const hasLayoutableShell = () => {
    const root = document.getElementById("root");
    if (!root?.firstElementChild) return false;
    const rect = root.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  };

  const signalReady = () => {
    cleanup();
    void signalChildWindowLifecycle({ phase: "shell-ready" }).catch(() => {});
  };

  const waitForMountedContent = () => {
    if (settled) return;
    if (hasLayoutableShell()) {
      waitForPaint();
      return;
    }
    contentPollTimeoutId = window.setTimeout(waitForMountedContent, 16);
  };

  const emitReady = () => {
    if (settled) return;
    if (!hasLayoutableShell()) {
      waitForMountedContent();
      return;
    }
    signalReady();
  };

  const emitReadyFromFallback = () => {
    if (settled) return;
    if (hasMountedShell()) {
      signalReady();
      return;
    }
    fallbackTimeoutId = window.setTimeout(emitReadyFromFallback, 16);
  };

  function waitForPaint() {
    if (settled) return;
    if (typeof window.requestAnimationFrame !== "function") {
      emitReady();
      return;
    }

    firstFrameId = window.requestAnimationFrame(() => {
      secondFrameId = window.requestAnimationFrame(emitReady);
    });
  }

  waitForMountedContent();
  fallbackTimeoutId = window.setTimeout(emitReadyFromFallback, 250);

  return cleanup;
}
