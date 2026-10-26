import { forwardRef, useEffect, useRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
import { LoaderCircle } from "lucide-react";

const focusRing =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  loading?: boolean;
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", loading, disabled, className = "", children, ...props },
  ref,
) {
  const styles = {
    primary: "bg-primary text-primary-foreground hover:opacity-90",
    secondary: "border border-border bg-background hover:bg-muted",
    ghost: "hover:bg-muted text-foreground",
    danger: "bg-danger-solid text-white hover:opacity-90",
  }[variant];
  return (
    <button
      ref={ref}
      disabled={disabled || loading}
      className={`inline-flex h-8 items-center justify-center gap-1.5 rounded-md px-3 text-[13px] font-medium transition-[background-color,opacity] disabled:opacity-50 ${styles} ${focusRing} ${className}`}
      {...props}
    >
      {loading && <LoaderCircle aria-hidden className="size-3.5 animate-spin" />}
      {children}
    </button>
  );
});

/** Small icon-only button. The label is required for screen readers. */
export const IconButton = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { label: string }>(function IconButton(
  { label, className = "", children, ...props },
  ref,
) {
  return (
    <button
      ref={ref}
      aria-label={label}
      title={label}
      className={`inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40 ${focusRing} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
});

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input(
  { className = "", ...props },
  ref,
) {
  return (
    <input
      ref={ref}
      autoCorrect="off"
      autoCapitalize="off"
      spellCheck={false}
      className={`h-8 w-full rounded-md border border-input bg-background px-2.5 text-[13px] placeholder:text-muted-foreground/70 aria-invalid:border-danger ${focusRing} ${className}`}
      {...props}
    />
  );
});

export function Field({
  label,
  htmlFor,
  hint,
  error,
  children,
}: {
  label: string;
  htmlFor: string;
  hint?: ReactNode;
  error?: string | null;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={htmlFor} className="text-xs font-medium text-muted-foreground">
        {label}
      </label>
      {children}
      {error ? (
        <p role="alert" className="text-xs text-danger">
          {error}
        </p>
      ) : (
        hint && <p className="text-xs text-muted-foreground">{hint}</p>
      )}
    </div>
  );
}

/** Confirmation dialog on the native <dialog>: Escape and focus trapping come free. */
export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel,
  onConfirm,
  onCancel,
  busy,
}: {
  open: boolean;
  title: string;
  body: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  busy?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      onCancel={(e) => {
        e.preventDefault();
        onCancel();
      }}
      className="m-auto w-[360px] rounded-xl border border-border bg-background p-5 text-foreground shadow-2xl backdrop:bg-black/30"
    >
      <h2 className="text-sm font-semibold">{title}</h2>
      <div className="mt-2 text-[13px] text-muted-foreground">{body}</div>
      <div className="mt-5 flex justify-end gap-2">
        <Button onClick={onCancel} autoFocus>
          Cancel
        </Button>
        <Button variant="danger" onClick={onConfirm} loading={busy}>
          {confirmLabel}
        </Button>
      </div>
    </dialog>
  );
}

/** Lets the window be dragged from this area (we hide the native title bar). */
export function DragRegion({ className = "" }: { className?: string }) {
  return <div data-tauri-drag-region className={className} />;
}
