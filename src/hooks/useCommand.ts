import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

export const NOT_AVAILABLE = "not-available";

interface UseCommandOptions {
  /** Re-run the command every `intervalMs` while the window is visible. */
  intervalMs?: number;
}

interface UseCommandResult<T> {
  data: T | null;
  error: string | null;
  /** True until the first response (or error) arrives. */
  loading: boolean;
  /** True while a manual refresh is in flight. */
  refreshing: boolean;
  refresh: () => Promise<void>;
}

/** Calls a Tauri command and keeps its latest result. */
export function useCommand<T>(command: string, { intervalMs }: UseCommandOptions = {}): UseCommandResult<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const inFlight = useRef(false);

  const run = useCallback(async () => {
    if (inFlight.current) return;
    if (!isTauri()) {
      setError(NOT_AVAILABLE);
      setLoading(false);
      return;
    }
    inFlight.current = true;
    try {
      setData(await invoke<T>(command));
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      inFlight.current = false;
      setLoading(false);
    }
  }, [command]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    await run();
    setRefreshing(false);
  }, [run]);

  useEffect(() => {
    run();
    if (!intervalMs) return;
    const id = window.setInterval(() => {
      if (!document.hidden) run();
    }, intervalMs);
    return () => window.clearInterval(id);
  }, [run, intervalMs]);

  return { data, error, loading, refreshing, refresh };
}
