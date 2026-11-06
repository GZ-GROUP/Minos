import { WarningCircle } from "@phosphor-icons/react";
import { describeError, errorDetail } from "../lib/errors";

/** A backend error: the friendly message, plus the exact cause for module authors. */
export function ErrorNote({ error }: { error: unknown }) {
  const detail = errorDetail(error);
  return (
    <div className="marketplace-error" role="alert">
      <WarningCircle weight="bold" aria-hidden="true" />
      <div>
        <p>{describeError(error)}</p>
        {detail && <code>{detail}</code>}
      </div>
    </div>
  );
}
