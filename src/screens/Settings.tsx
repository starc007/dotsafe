import { useCallback, useEffect, useRef, useState } from "react";
import { Keyboard, Laptop, Lock, Moon, Palette, ShieldCheck, Sun, X } from "lucide-react";
import { api, message, type Settings } from "@/lib/api";
import { displayPath } from "@/lib/paths";
import { SHORTCUTS, ariaShortcut, type Shortcut } from "@/lib/shortcuts";
import { useTheme, type ThemePreference } from "@/lib/theme";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { DragRegion } from "@/components/DragRegion";
import { LicenseCard } from "@/components/LicenseCard";
import { SettingsRow } from "@/components/SettingsRow";
import { ShortcutKeys } from "@/components/ShortcutKeys";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

const SHORTCUT_GROUPS = ["General", "Projects", "Variables"] as const;

/** Settings for this Mac. Fully keyboard-driven: Tab through controls, Esc closes. */
export function SettingsPage({ onClose, onLock }: { onClose: () => void; onLock: () => void }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { preference, setPreference } = useTheme();
  const scroller = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    try {
      setSettings(await api.getSettings());
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  }, []);

  useEffect(() => {
    load();
    // Start keyboard focus inside the page, so Tab walks its controls in order.
    scroller.current?.focus();
  }, [load]);

  // Esc closes settings. In a field with text, the first Esc only leaves the field.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"], [role="menu"][data-state="open"]')) return;
      const target = e.target as HTMLElement;
      if ((target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) && target.value) {
        target.blur();
        scroller.current?.focus();
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
        <h1 data-tauri-drag-region className="text-sm font-semibold">
          Settings
        </h1>
        <DragRegion className="flex-1 self-stretch" />
        <Button variant="ghost" size="sm" onClick={onClose} aria-keyshortcuts="Escape" className="text-muted-foreground">
          <X aria-hidden />
          Close
          <Kbd aria-hidden>Esc</Kbd>
        </Button>
      </header>

      <div ref={scroller} tabIndex={-1} className="relative min-h-0 flex-1 overflow-y-auto p-4 outline-none">
        <div className="mx-auto flex max-w-2xl flex-col gap-4 pb-4">
          {error ? (
            <div className="flex flex-col items-center gap-3 py-16 text-center">
              <p className="text-sm font-semibold">Couldn't load settings</p>
              <p className="selectable text-muted-foreground">{error}</p>
              <Button variant="outline" onClick={load}>
                Try again
              </Button>
            </div>
          ) : !settings ? (
            <SettingsSkeleton />
          ) : (
            <>
              <LicenseCard licenseKey={settings.licenseKey} onChanged={load} />

              <Surface aria-labelledby="appearance-title">
                <SurfaceHeader icon={<Palette aria-hidden />}>
                  <SurfaceTitle id="appearance-title">Appearance</SurfaceTitle>
                  <SurfaceDescription>Light, dark, or match your Mac.</SurfaceDescription>
                </SurfaceHeader>
                <SurfaceBody>
                  <SettingsRow label="Theme" detail="Applies right away and is remembered on this Mac.">
                    <Tabs variant="segment" value={preference} onValueChange={(v) => setPreference(v as ThemePreference)}>
                      <TabsList aria-label="Theme" className="bg-muted p-0.5">
                        <TabsTrigger value="system" icon={<Laptop className="size-3.5" />} className="px-2.5 py-1 text-[13px]">
                          System
                        </TabsTrigger>
                        <TabsTrigger value="light" icon={<Sun className="size-3.5" />} className="px-2.5 py-1 text-[13px]">
                          Light
                        </TabsTrigger>
                        <TabsTrigger value="dark" icon={<Moon className="size-3.5" />} className="px-2.5 py-1 text-[13px]">
                          Dark
                        </TabsTrigger>
                      </TabsList>
                    </Tabs>
                  </SettingsRow>
                </SurfaceBody>
              </Surface>

              <Surface aria-labelledby="security-title">
                <SurfaceHeader icon={<ShieldCheck aria-hidden />}>
                  <SurfaceTitle id="security-title">Vault and security</SurfaceTitle>
                  <SurfaceDescription>Where your vault lives and how dotsafe protects it.</SurfaceDescription>
                </SurfaceHeader>
                <SurfaceBody className="divide-y">
                  <SettingsRow label="Vault location" detail="Synced by the service that owns this folder.">
                    <span title={settings.vaultPath ?? undefined} className="selectable block max-w-56 truncate">
                      {settings.vaultPath ? displayPath(settings.vaultPath) : "Not set up"}
                    </span>
                  </SettingsRow>
                  <SettingsRow label="Auto-lock" detail="Locks when you haven't used dotsafe for a while.">
                    After {settings.autoLockMinutes} minutes
                  </SettingsRow>
                  <SettingsRow label="Clipboard" detail="Copied values are cleared unless you've copied something else since.">
                    After {settings.clipboardClearSeconds} seconds
                  </SettingsRow>
                  <SettingsRow label="Lock now" detail="You'll need your master password to get back in.">
                    <Button variant="outline" onClick={onLock} aria-keyshortcuts={ariaShortcut(SHORTCUTS.lock)}>
                      <Lock aria-hidden />
                      Lock
                      <ShortcutKeys shortcut={SHORTCUTS.lock} />
                    </Button>
                  </SettingsRow>
                </SurfaceBody>
              </Surface>

              <Surface aria-labelledby="shortcuts-title">
                <SurfaceHeader icon={<Keyboard aria-hidden />}>
                  <SurfaceTitle id="shortcuts-title">Keyboard shortcuts</SurfaceTitle>
                  <SurfaceDescription>Everything in dotsafe works from the keyboard.</SurfaceDescription>
                </SurfaceHeader>
                <SurfaceBody className="flex flex-col gap-5">
                  {SHORTCUT_GROUPS.map((group) => (
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
                          <div className="flex items-center justify-between py-2">
                            <dt>Close a dialog or settings</dt>
                            <dd>
                              <Kbd>Esc</Kbd>
                            </dd>
                          </div>
                        )}
                      </dl>
                    </section>
                  ))}
                </SurfaceBody>
              </Surface>

              <p className="text-center text-xs text-muted-foreground">dotsafe {settings.version}</p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function SettingsSkeleton() {
  return (
    <div aria-hidden className="flex flex-col gap-4">
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-36 rounded-3xl" />
      ))}
    </div>
  );
}
