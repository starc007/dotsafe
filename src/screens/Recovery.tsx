import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "../components/ui";
import { Centered } from "../components/Centered";

export function Recovery({ recoveryKey, onDone }: { recoveryKey: string; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);
  const groups = recoveryKey.split("-");

  async function copy() {
    await navigator.clipboard.writeText(recoveryKey);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <Centered
      title="Save your recovery key"
      subtitle="If you forget your master password, this key is the only way back into your vault. It won't be shown again."
    >
      <div className="flex flex-col gap-4 rounded-xl border border-border bg-background p-5 shadow-sm">
        <div
          aria-label="Recovery key"
          className="selectable grid grid-cols-4 gap-x-3 gap-y-2 rounded-lg bg-surface p-4 font-mono text-[13px] tracking-wider"
        >
          {groups.map((g, i) => (
            <span key={i}>{g}</span>
          ))}
        </div>
        <Button type="button" onClick={copy} className="self-start">
          {copied ? <Check aria-hidden className="size-3.5 text-success" /> : <Copy aria-hidden className="size-3.5" />}
          {copied ? "Copied" : "Copy key"}
        </Button>
        <p className="text-xs text-muted-foreground">
          Store it offline, such as printed or in a password manager on another device. Not in the same folder as your vault.
        </p>
        <label className="flex items-start gap-2 text-[13px]">
          <input
            type="checkbox"
            className="mt-0.5 size-3.5 accent-primary"
            checked={saved}
            onChange={(e) => setSaved(e.target.checked)}
          />
          I've saved my recovery key somewhere safe
        </label>
        <Button variant="primary" className="w-full" disabled={!saved} onClick={onDone}>
          Continue
        </Button>
      </div>
    </Centered>
  );
}
