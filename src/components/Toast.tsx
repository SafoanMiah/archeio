import { AlertCircle, Info, X } from "lucide-react";
import { cx } from "@/lib/utils";

export type ToastKind = "error" | "info";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

interface Props {
  toast: ToastItem;
  onDismiss: (id: number) => void;
}

export function Toast({ toast, onDismiss }: Props) {
  const isError = toast.kind === "error";
  return (
    <div
      className={cx(
        "pointer-events-auto flex w-80 items-start gap-3 rounded-md border p-3 shadow-lg toast-enter",
        isError
          ? "border-red-900/60 bg-red-950/70 text-red-100"
          : "border-zinc-800 bg-zinc-900 text-zinc-100",
      )}
      role="alert"
    >
      <div className="mt-0.5 shrink-0">
        {isError ? (
          <AlertCircle className="h-4 w-4 text-accent" />
        ) : (
          <Info className="h-4 w-4 text-zinc-400" />
        )}
      </div>
      <div className="flex-1 text-sm leading-snug break-words">{toast.message}</div>
      <button
        onClick={() => onDismiss(toast.id)}
        className="shrink-0 text-zinc-400 hover:text-zinc-200"
        aria-label="Dismiss"
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  );
}
