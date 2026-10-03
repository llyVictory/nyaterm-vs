import { listen } from "@/lib/backend/api";
import { useEffect, useRef } from "react";
import {
  signalChildWindowCommandReady,
  signalChildWindowLoadFailed,
} from "@/lib/childWindowLifecycle";
import type { ChildWindowCommandName } from "@/lib/childWindowProtocol";

/**
 * 注册子窗口业务命令，并在当前 effect 的 listener 确认可用后报告 ready。
 * active 标记会忽略 StrictMode 首轮已清理的异步注册，避免在有效 listener 建立前释放队列。
 */
export function useChildWindowCommand<T>(
  event: ChildWindowCommandName,
  handler: (payload: T) => void,
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let active = true;
    let dispose: (() => void) | undefined;

    void listen<T>(event, ({ payload }) => handlerRef.current(payload))
      .then((unlisten) => {
        if (!active) {
          unlisten();
          return;
        }
        dispose = unlisten;
        void signalChildWindowCommandReady(event).catch(() => {});
      })
      .catch(() => {
        if (active) void signalChildWindowLoadFailed("command-listener").catch(() => {});
      });

    return () => {
      active = false;
      dispose?.();
    };
  }, [event]);
}
