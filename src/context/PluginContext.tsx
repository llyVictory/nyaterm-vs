import { listen } from "@/lib/backend/api";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { getErrorMessage } from "@/lib/errors";
import {
  PLUGIN_OPEN_EVENT,
  pluginApi,
  type PluginOpenIntent,
} from "@/lib/plugins";
import type { InstalledPlugin } from "@/types/plugins";

type PluginContextValue = {
  plugins: InstalledPlugin[];
  loaded: boolean;
  error: string | null;
  generation: number;
  locked: boolean;
  openIntent: PluginOpenIntent | null;
  refresh: () => Promise<void>;
};

const PluginContext = createContext<PluginContextValue>({
  plugins: [],
  loaded: false,
  error: null,
  generation: 0,
  locked: false,
  openIntent: null,
  refresh: async () => {},
});

export const usePlugins = () => useContext(PluginContext);

export function PluginProvider({ children }: { children: React.ReactNode }) {
  const [plugins, setPlugins] = useState<InstalledPlugin[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generation, setGeneration] = useState(0);
  const [locked, setLocked] = useState(false);
  const [openIntent, setOpenIntent] = useState<PluginOpenIntent | null>(null);
  const sequence = useRef(0);
  const mounted = useRef(false);
  const refresh = useCallback(async () => {
    const request = ++sequence.current;
    try {
      const next = await pluginApi.list();
      if (!mounted.current || request !== sequence.current) return;
      setPlugins(next);
      setError(null);
    } catch (error) {
      if (!mounted.current || request !== sequence.current) return;
      setError(getErrorMessage(error));
    } finally {
      if (mounted.current && request === sequence.current) setLoaded(true);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    const disposers: (() => void)[] = [];
    let disposed = false;
    const register = (promise: Promise<() => void>) => {
      void promise
        .then((dispose) => (disposed ? dispose() : disposers.push(dispose)))
        .catch(() => {});
    };
    register(
      listen("plugins-changed", () => {
        setGeneration((value) => value + 1);
        void refresh();
      }),
    );
    register(
      listen<{ locked: boolean }>("app-lock-state-changed", (event) => {
        setLocked(event.payload.locked);
        setGeneration((value) => value + 1);
      }),
    );
    void refresh();
    const open = (event: Event) =>
      setOpenIntent((event as CustomEvent<PluginOpenIntent>).detail);
    window.addEventListener(PLUGIN_OPEN_EVENT, open);
    return () => {
      mounted.current = false;
      disposed = true;
      sequence.current += 1;
      for (const dispose of disposers) dispose();
      window.removeEventListener(PLUGIN_OPEN_EVENT, open);
    };
  }, [refresh]);

  return (
    <PluginContext.Provider
      value={{
        plugins,
        loaded,
        error,
        generation,
        locked,
        openIntent,
        refresh,
      }}
    >
      {children}
    </PluginContext.Provider>
  );
}
