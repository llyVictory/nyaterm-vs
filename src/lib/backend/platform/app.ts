import * as native from "@tauri-apps/api/app";
import { runtime } from "../runtime";
export const getName: typeof native.getName = () =>
  runtime === "desktop" ? native.getName() : Promise.resolve("NyaTerm Web");
export const getVersion: typeof native.getVersion = () =>
  runtime === "desktop"
    ? native.getVersion()
    : Promise.resolve(__APP_VERSION__);
