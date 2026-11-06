import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { NOT_AVAILABLE } from "../hooks/useCommand";
import { errorDetail } from "../lib/errors";
import { marketplace, type Catalog, type MarketplaceData } from "./api";

/**
 * Marketplace state. Lives above the modal so that a download keeps running, and
 * stays visible, after the modal is closed.
 */
export function useMarketplace() {
  const [data, setData] = useState<MarketplaceData | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  /** Active downloads: id → percent, or null while the size is unknown. */
  const [downloads, setDownloads] = useState<Record<string, number | null>>({});
  /** Last failure per module, shown on its card. */
  const [errors, setErrors] = useState<Record<string, unknown>>({});
  const catalogRequested = useRef(false);

  useEffect(() => {
    if (!isTauri()) {
      setLoadError(NOT_AVAILABLE);
      return;
    }
    marketplace
      .state()
      .then(setData)
      .catch((cause) => setLoadError(errorDetail(cause) ?? String(cause)));
  }, []);

  /** Reads the catalog the first time the Marketplace opens, not at startup. */
  const loadCatalog = useCallback(() => {
    if (catalogRequested.current || !isTauri()) return;
    catalogRequested.current = true;
    marketplace
      .catalog()
      .then(setCatalog)
      .catch(() => setCatalog({ modules: [], offline: true }));
  }, []);

  const run = useCallback(async (id: string, action: () => Promise<MarketplaceData>) => {
    setErrors(({ [id]: _cleared, ...rest }) => rest);
    try {
      setData(await action());
    } catch (cause) {
      setErrors((current) => ({ ...current, [id]: cause }));
    }
  }, []);

  const setInstalled = useCallback(
    (id: string, installed: boolean) => run(id, () => marketplace.setInstalled(id, installed)),
    [run],
  );

  /** Applied at once so the home screen doesn't jump; reverts if it can't be saved. */
  const reorder = useCallback((ids: string[]) => {
    setData((current) => current && { ...current, installed: ids });
    marketplace
      .setOrder(ids)
      .then(setData)
      .catch(() => marketplace.state().then(setData));
  }, []);

  const remove = useCallback((id: string) => run(id, () => marketplace.remove(id)), [run]);

  const download = useCallback(
    async (repo: string, id: string) => {
      setDownloads((current) => ({ ...current, [id]: null }));
      await run(id, () =>
        marketplace.download(repo, id, (percent) => setDownloads((current) => ({ ...current, [id]: percent }))),
      );
      setDownloads(({ [id]: _finished, ...rest }) => rest);
    },
    [run],
  );

  /** These reject on failure: the dialog that started them shows the error. */
  const addRepository = useCallback(async (url: string) => setData(await marketplace.addRepository(url)), []);
  const addZip = useCallback(async (path: string) => setData(await marketplace.addZip(path)), []);

  return {
    data,
    loadError,
    catalog,
    downloads,
    errors,
    loadCatalog,
    setInstalled,
    reorder,
    remove,
    download,
    addRepository,
    addZip,
  };
}

export type Marketplace = ReturnType<typeof useMarketplace>;
