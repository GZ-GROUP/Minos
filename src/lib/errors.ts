import i18n from "../i18n";

/** Shape of `AppError` returned by every Rust command (src-tauri/src/apps/error.rs). */
export interface AppError {
  code: string;
  message: string;
}

export function errorCode(error: unknown): string {
  const code = (error as Partial<AppError> | null)?.code;
  return typeof code === "string" ? code : "io";
}

/** Technical detail from the backend, when there is one. */
export function errorDetail(error: unknown): string | undefined {
  const message = (error as Partial<AppError> | null)?.message;
  return typeof message === "string" && message ? message : undefined;
}

/** Friendly, translated message for any command error (common "errors" keys). */
export function describeError(error: unknown): string {
  return i18n.t(`errors.${errorCode(error)}`, { defaultValue: i18n.t("errors.io") });
}
