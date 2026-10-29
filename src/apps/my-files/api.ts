import { invoke } from "@tauri-apps/api/core";

export type FsErrorCode =
  | "not_found"
  | "exists"
  | "permission_denied"
  | "invalid_name"
  | "outside_home"
  | "not_empty"
  | "io";

/** Error shape returned by the Rust commands (src-tauri/src/apps/my_files.rs). */
export interface FsError {
  code: FsErrorCode;
  message: string;
}

export interface FsEntry {
  name: string;
  path: string;
  kind: "dir" | "file";
  size: number;
  modifiedMs: number | null;
  hidden: boolean;
}

export interface Listing {
  path: string;
  crumbs: { name: string; path: string }[];
  entries: FsEntry[];
}

export const files = {
  home: () => invoke<string>("files_home"),
  list: (home: string, path: string, showHidden: boolean) =>
    invoke<Listing>("files_list", { home, path, showHidden }),
  createFile: (home: string, parent: string, name: string) =>
    invoke<string>("files_create_file", { home, parent, name }),
  createDir: (home: string, parent: string, name: string) =>
    invoke<string>("files_create_dir", { home, parent, name }),
  rename: (home: string, path: string, newName: string) =>
    invoke<string>("files_rename", { home, path, newName }),
  remove: (home: string, path: string) => invoke<void>("files_delete", { home, path }),
  permissions: (home: string, path: string) => invoke<FilePermissions>("files_permissions", { home, path }),
  setPermissions: (home: string, path: string, next: FilePermissions) =>
    invoke<FilePermissions>("files_set_permissions", { home, path, ...next }),
};

/** Effective access for the current user. `execute` is null on Windows. */
export interface FilePermissions {
  read: boolean;
  write: boolean;
  execute: boolean | null;
}

export function errorCode(error: unknown): FsErrorCode {
  const code = (error as Partial<FsError> | null)?.code;
  return typeof code === "string" ? code : "io";
}

const INVALID_CHARS = /[\\/:*?"<>|\u0000-\u001f]/;
const RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i;

/**
 * Mirrors the backend rules so most mistakes are caught before calling it.
 * Returns an i18n key (in the "my-files" namespace) or null when valid.
 */
export function validateName(name: string): string | null {
  if (!name.trim()) return "validation.empty";
  if (name === "." || name === "..") return "validation.reserved";
  if (INVALID_CHARS.test(name)) return "validation.chars";
  if (/^\s|[.\s]$/.test(name)) return "validation.edges";
  if (RESERVED.test(name)) return "validation.reserved";
  return null;
}
