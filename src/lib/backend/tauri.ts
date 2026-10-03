import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
// Access native functions at call time. This also preserves the existing
// Desktop tests that mock only the API functions they exercise.
export const tauriBackend = {
  invoke: (...args: Parameters<typeof invoke>) => invoke(...args),
  emit: (...args: Parameters<typeof emit>) => emit(...args),
  listen: (...args: Parameters<typeof listen>) => listen(...args),
} as { invoke: typeof invoke; emit: typeof emit; listen: typeof listen };
