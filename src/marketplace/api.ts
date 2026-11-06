import { Channel, invoke } from "@tauri-apps/api/core";
import type { AppCategory, Platform } from "../apps/types";

/** Text per language code; `en` is always present. */
export type Localized = Record<string, string>;

/** Backend counterparts live in src-tauri/src/marketplace/. */
export interface ModuleManifest {
  id: string;
  version: string;
  name: Localized;
  description: Localized;
  author: string;
  category: AppCategory;
  platforms: Platform[];
}

export interface LocalModule {
  manifest: ModuleManifest;
  /** Repository link; null for a package loaded from a local .zip. */
  repo: string | null;
  downloaded: boolean;
}

export interface MarketplaceData {
  platform: Platform;
  /** Integrated apps shown on the home screen, in the user's order. */
  installed: string[];
  modules: LocalModule[];
}

export interface CatalogEntry {
  id: string;
  repo: string;
  name: Localized;
  description: Localized;
  author: string;
  category: AppCategory;
  platforms: Platform[];
}

export interface Catalog {
  modules: CatalogEntry[];
  /** The online catalog couldn't be read, so the bundled copy is shown. */
  offline: boolean;
}

interface DownloadProgress {
  percent: number | null;
}

export const marketplace = {
  state: () => invoke<MarketplaceData>("marketplace_state"),
  catalog: () => invoke<Catalog>("marketplace_catalog"),
  setInstalled: (id: string, installed: boolean) =>
    invoke<MarketplaceData>("marketplace_set_installed", { id, installed }),
  setOrder: (ids: string[]) => invoke<MarketplaceData>("marketplace_set_order", { ids }),
  addRepository: (url: string) => invoke<MarketplaceData>("marketplace_add_repository", { url }),
  addZip: (path: string) => invoke<MarketplaceData>("marketplace_add_zip", { path }),
  download: (repo: string, id: string, onProgress: (percent: number | null) => void) => {
    const channel = new Channel<DownloadProgress>();
    channel.onmessage = (progress) => onProgress(progress.percent);
    return invoke<MarketplaceData>("marketplace_download", { repo, id, onProgress: channel });
  },
  remove: (id: string) => invoke<MarketplaceData>("marketplace_remove", { id }),
};
