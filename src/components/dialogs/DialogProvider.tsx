import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { InputBox, type FormValues, type PromptOptions } from "./InputBox";
import { OptionBox, type ConfirmOptions } from "./OptionBox";
import { MessageBox, type AlertOptions } from "./MessageBox";

interface DialogApi {
  /** InputBox. Resolves with the field values, or null if cancelled. */
  prompt: (options: PromptOptions) => Promise<FormValues | null>;
  /** OptionBox. Resolves with the chosen option id, or null if dismissed. */
  confirm: (options: ConfirmOptions) => Promise<string | null>;
  /** MessageBox. Resolves when closed. */
  alert: (options: AlertOptions) => Promise<void>;
}

type Request =
  | { kind: "prompt"; id: number; options: PromptOptions; resolve: (value: FormValues | null) => void }
  | { kind: "confirm"; id: number; options: ConfirmOptions; resolve: (value: string | null) => void }
  | { kind: "alert"; id: number; options: AlertOptions; resolve: () => void };

const DialogContext = createContext<DialogApi | null>(null);

/** Hosts one dialog at a time and exposes the promise-based `useDialogs()` API. */
export function DialogProvider({ children }: { children: ReactNode }) {
  const [request, setRequest] = useState<Request | null>(null);
  const counter = useRef(0);

  // A new request replaces (and cancels) the current one.
  const open = useCallback((next: Omit<Request, "id">) => {
    setRequest((current) => {
      if (current?.kind === "alert") current.resolve();
      else current?.resolve(null);
      return { ...next, id: ++counter.current } as Request;
    });
  }, []);

  const api = useMemo<DialogApi>(
    () => ({
      prompt: (options) => new Promise((resolve) => open({ kind: "prompt", options, resolve })),
      confirm: (options) => new Promise((resolve) => open({ kind: "confirm", options, resolve })),
      alert: (options) => new Promise((resolve) => open({ kind: "alert", options, resolve })),
    }),
    [open],
  );

  const finish = <T,>(resolve: (value: T) => void) => (value: T) => {
    setRequest(null);
    resolve(value);
  };

  return (
    <DialogContext.Provider value={api}>
      {children}
      {request?.kind === "prompt" && (
        <InputBox key={request.id} options={request.options} onResolve={finish(request.resolve)} />
      )}
      {request?.kind === "confirm" && (
        <OptionBox key={request.id} options={request.options} onResolve={finish(request.resolve)} />
      )}
      {request?.kind === "alert" && (
        <MessageBox key={request.id} options={request.options} onResolve={() => finish(request.resolve)(undefined)} />
      )}
    </DialogContext.Provider>
  );
}

export function useDialogs(): DialogApi {
  const context = useContext(DialogContext);
  if (!context) throw new Error("useDialogs must be used within DialogProvider");
  return context;
}
