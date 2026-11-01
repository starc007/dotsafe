import { Keyboard } from "lucide-react";
import { SHORTCUTS, ariaShortcut, isMac, type Shortcut } from "@/lib/shortcuts";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { ShortcutKeys } from "@/components/ShortcutKeys";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

const GROUPS = ["General", "Projects", "Variables"] as const;

export function ShortcutsSettings() {
  return (
    <Surface aria-labelledby="shortcuts-title">
      <SurfaceHeader icon={<Keyboard aria-hidden />}>
        <SurfaceTitle id="shortcuts-title">Keyboard shortcuts</SurfaceTitle>
        <SurfaceDescription>⌘ on macOS, Ctrl on Windows and Linux.</SurfaceDescription>
      </SurfaceHeader>
      <SurfaceBody className="flex flex-col gap-5">
        {GROUPS.map((group) => (
          <section key={group} aria-labelledby={`shortcuts-${group}`}>
            <h3 id={`shortcuts-${group}`} className="mb-1 text-xs font-medium text-muted-foreground">
              {group}
            </h3>
            <dl className="divide-y">
              {(Object.values(SHORTCUTS) as Shortcut[])
                .filter((s) => s.group === group)
                .map((s) => (
                  <div key={s.label} className="flex items-center justify-between py-2">
                    <dt>{s.label}</dt>
                    <dd>
                      <span className="sr-only">{ariaShortcut(s)}</span>
                      <ShortcutKeys shortcut={s} />
                    </dd>
                  </div>
                ))}
              {group === "General" && (
                <>
                  <div className="flex items-center justify-between py-2">
                    <dt>Find and copy a variable from any app</dt>
                    <dd>
                      <span className="sr-only">{isMac ? "Meta+Alt+K" : "Control+Alt+K"}</span>
                      <KbdGroup aria-hidden>
                        <Kbd>{isMac ? "⌥" : "Alt"}</Kbd>
                        <Kbd>{isMac ? "⌘" : "Ctrl"}</Kbd>
                        <Kbd>K</Kbd>
                      </KbdGroup>
                    </dd>
                  </div>
                  <div className="flex items-center justify-between py-2">
                    <dt>Close a dialog or settings</dt>
                    <dd>
                      <Kbd>Esc</Kbd>
                    </dd>
                  </div>
                </>
              )}
            </dl>
          </section>
        ))}
      </SurfaceBody>
    </Surface>
  );
}
