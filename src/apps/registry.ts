import { lazy, type ComponentType } from "react";
import i18n from "../i18n";
import type { AppDefinition, AppLocale, AppManifest } from "./types";

/*
 * Apps are discovered automatically: every folder in `src/apps/` containing a
 * `manifest.ts` and an `index.tsx` becomes an entry in the drawer.
 */

const manifests = import.meta.glob<AppManifest>("./*/manifest.ts", {
  eager: true,
  import: "default",
});

const locales = import.meta.glob<AppLocale>("./*/locales/*.json", {
  eager: true,
  import: "default",
});

const components = import.meta.glob<{ default: ComponentType }>("./*/index.tsx");

function folderOf(path: string): string {
  return path.split("/")[1];
}

// Register each app's translations under its own namespace (= app id).
for (const [path, strings] of Object.entries(locales)) {
  const lang = path.split("/").pop()!.replace(".json", "");
  i18n.addResourceBundle(lang, folderOf(path), strings, true, true);
}

export const apps: AppDefinition[] = Object.entries(manifests)
  .flatMap(([path, manifest]) => {
    const folder = folderOf(path);
    const loader = components[`./${folder}/index.tsx`];
    if (!loader) {
      console.warn(`[apps] "${folder}" has a manifest but no index.tsx; skipping`);
      return [];
    }
    if (manifest.id !== folder) {
      console.warn(`[apps] manifest id "${manifest.id}" does not match folder "${folder}"`);
    }
    return [
      {
        id: folder,
        icon: manifest.icon,
        order: manifest.order ?? Number.MAX_SAFE_INTEGER,
        Component: lazy(loader),
      },
    ];
  })
  .sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));
