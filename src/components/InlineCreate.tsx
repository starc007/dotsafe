import { useState, type FormEvent } from "react";
import { message } from "../lib/api";
import { Input } from "./ui";

/** A one-field form that creates something on Enter and cancels on Escape or blur. */
export function InlineCreate({
  label,
  placeholder,
  className = "",
  onCreate,
  onCancel,
}: {
  label: string;
  placeholder?: string;
  className?: string;
  onCreate: (name: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!name.trim()) return onCancel();
    setBusy(true);
    try {
      await onCreate(name.trim());
    } catch (err) {
      setError(message(err));
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className={className}>
      <Input
        aria-label={label}
        placeholder={placeholder}
        autoFocus
        disabled={busy}
        value={name}
        aria-invalid={!!error}
        onChange={(e) => {
          setName(e.target.value);
          setError(null);
        }}
        onKeyDown={(e) => e.key === "Escape" && onCancel()}
        onBlur={() => !name.trim() && onCancel()}
        className="h-7"
      />
      {error && (
        <p role="alert" className="mt-1 px-1 text-xs text-danger">
          {error}
        </p>
      )}
    </form>
  );
}
