import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { cn } from "@/lib/utils";
import { shortcutKeys, type Shortcut } from "@/lib/shortcuts";

/** A shortcut as key caps, e.g. ⌘ ⇧ N. Decorative: pair with aria-keyshortcuts. */
export function ShortcutKeys({ shortcut, className }: { shortcut: Shortcut; className?: string }) {
  return (
    <KbdGroup aria-hidden className={cn(className)}>
      {shortcutKeys(shortcut).map((key) => (
        <Kbd key={key}>{key}</Kbd>
      ))}
    </KbdGroup>
  );
}
