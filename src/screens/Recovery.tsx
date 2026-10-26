import { useState } from "react";
import { Check, Copy, KeyRound } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Centered } from "@/components/Centered";
import { SurfaceFooter } from "@/components/Surface";
import { ActionSwapIcon, ActionSwapText } from "@/components/motion/action-swap";

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
      icon={<KeyRound aria-hidden />}
      title="Save your recovery key"
      subtitle="The only way back in if you forget your password."
    >
      <div className="flex flex-col gap-4">
        <div
          aria-label="Recovery key"
          className="selectable grid grid-cols-4 gap-x-3 gap-y-2 rounded-2xl bg-muted/60 p-4 font-mono text-[13px] tracking-wider"
        >
          {recoveryKey.split("-").map((group, i) => (
            <span key={i}>{group}</span>
          ))}
        </div>
        <Button type="button" variant="outline" onClick={copy} className="self-start">
          <ActionSwapIcon value={copied ? "copied" : "copy"} animation="blur" className="size-4">
            {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
          </ActionSwapIcon>
          <ActionSwapText value={copied ? "copied" : "copy"} animation="blur">
            {copied ? "Copied" : "Copy key"}
          </ActionSwapText>
        </Button>
        <p className="text-xs text-muted-foreground">
          It won't be shown again. Store it offline, such as printed or in a password manager on another device, not
          in the same folder as your vault.
        </p>
        <div className="flex items-start gap-2">
          <Checkbox id="saved" checked={saved} onCheckedChange={(v) => setSaved(v === true)} className="mt-px" />
          <Label htmlFor="saved" className="font-normal leading-snug">
            I've saved my recovery key somewhere safe
          </Label>
        </div>
        <SurfaceFooter className="pt-1">
          <Button size="lg" className="px-4" disabled={!saved} onClick={onDone}>
            Continue
          </Button>
        </SurfaceFooter>
      </div>
    </Centered>
  );
}
