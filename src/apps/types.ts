import type { ComponentType, LazyExoticComponent } from "react";
import type { IconProps } from "@phosphor-icons/react";

/**
 * Icon component for an app. Usually a Phosphor icon; custom icons must accept
 * IconProps and draw with `currentColor` so they follow the active theme.
 */
export type AppIcon = ComponentType<IconProps>;

/** Marketplace categories. Modules use the same list (src-tauri/src/marketplace/manifest.rs). */
export type AppCategory = "documents" | "files" | "system" | "network" | "utilities";

export type Platform = "windows" | "macos" | "linux";

/** Shape of each app folder's `manifest.ts` default export. */
export interface AppManifest {
  /** Unique id; must match the folder name. Also used as the app's i18n namespace. */
  id: string;
  icon: AppIcon;
  category: AppCategory;
  /** Systems the app works on. Omit when it works on all of them. */
  platforms?: Platform[];
}

/** Strings every app must provide in `locales/<lang>.json`. */
export interface AppLocale {
  name: string;
  description: string;
}

export interface AppDefinition {
  id: string;
  icon: AppIcon;
  category: AppCategory;
  platforms?: Platform[];
  Component: LazyExoticComponent<ComponentType>;
}
