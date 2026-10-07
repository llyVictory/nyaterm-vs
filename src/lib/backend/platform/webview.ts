import * as native from "@tauri-apps/api/webview";
import { runtime } from "../runtime";
export const getCurrentWebview: typeof native.getCurrentWebview = () =>
  runtime === "desktop"
    ? native.getCurrentWebview()
    : ({
        label: "main",
        onDragDropEvent: async () => () => {},
      } as unknown as ReturnType<typeof native.getCurrentWebview>);
