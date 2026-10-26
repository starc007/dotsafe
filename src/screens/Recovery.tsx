import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Centered, Panel } from "@/components/Centered";

export function Recovery({ recoveryKey, onDone }: { recoveryKey: string; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);

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
      <Panel>
        <div
          aria-label="Recovery key"
          className="selectable grid grid-cols-4 gap-x-3 gap-y-2 rounded-lg bg-muted/60 p-4 font-mono text-[13px] tracking-wider"
        >
          {recoveryKey.split("-").map((group, i) => (
            <span key={i}>{group}</span>
          ))}
        </div>
        <Button type="button" variant="outline" onClick={copy} className="self-start">
          {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
          {copied ? "Copied" : "Copy key"}
        </Button>
        <p className="text-xs text-muted-foreground">
          Store it offline, such as printed or in a password manager on another device. Not in the same folder as
          your vault.
        </p>
        <div className="flex items-start gap-2">
          <Checkbox id="saved" checked={saved} onCheckedChange={(v) => setSaved(v === true)} className="mt-px" />
          <Label htmlFor="saved" className="font-normal leading-snug">
            I've saved my recovery key somewhere safe
          </Label>
        </div>
        <Button className="w-full" disabled={!saved} onClick={onDone}>
          Continue
        </Button>
      </Panel>
    </Centered>
  );
}
