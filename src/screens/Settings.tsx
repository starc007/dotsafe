import { useCallback, useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { X } from "lucide-react";
import { api, message, type Settings } from "@/lib/api";
import { fast } from "@/lib/motion";
import { SETTINGS_SECTIONS, type SettingsSection } from "@/lib/settingsSections";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Skeleton } from "@/components/ui/skeleton";
import { DragRegion } from "@/components/DragRegion";
import { LicenseCard } from "@/components/LicenseCard";
import { AppearanceSettings } from "@/components/settings/AppearanceSettings";
import { SecuritySettings } from "@/components/settings/SecuritySettings";
import { ShortcutsSettings } from "@/components/settings/ShortcutsSettings";

/** One settings section, chosen in the sidebar (SettingsNav). Esc closes. */
export function SettingsPage({
  section,
  onClose,
  onLock,
}: {
  section: SettingsSection;
  onClose: () => void;
  onLock: () => void;
}) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const meta = SETTINGS_SECTIONS.find((s) => s.id === section)!;

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
  }, [load]);

  // Esc closes settings. In a field with text, the first Esc only leaves the field.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"], [role="menu"][data-state="open"]')) return;
      const target = e.target as HTMLElement;
      if ((target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) && target.value) {
        target.blur();
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
          {meta.label}
        </h1>
        <DragRegion className="flex-1 self-stretch" />
        <Button variant="ghost" size="sm" onClick={onClose} aria-keyshortcuts="Escape" className="text-muted-foreground">
          <X aria-hidden />
          Close
          <Kbd aria-hidden>Esc</Kbd>
        </Button>
      </header>

      <div className="relative min-h-0 flex-1 overflow-y-auto p-4">
        <div className="mx-auto max-w-2xl pb-4">
          {error ? (
            <div className="flex flex-col items-center gap-3 py-16 text-center">
              <p className="text-sm font-semibold">Couldn't load settings</p>
              <p className="selectable text-muted-foreground">{error}</p>
              <Button variant="outline" onClick={load}>
                Try again
              </Button>
            </div>
          ) : !settings ? (
            <Skeleton aria-hidden className="h-40 rounded-3xl" />
          ) : (
            <AnimatePresence mode="wait" initial={false}>
              <motion.div
                key={section}
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
                transition={fast}
                className="flex flex-col gap-4"
              >
                <p className="px-1 text-muted-foreground">{meta.description}</p>
                {section === "license" && <LicenseCard licenseKey={settings.licenseKey} onChanged={load} />}
                {section === "appearance" && <AppearanceSettings />}
                {section === "security" && <SecuritySettings settings={settings} onLock={onLock} />}
                {section === "shortcuts" && <ShortcutsSettings />}
                <p className="pt-2 text-center text-xs text-muted-foreground">dotsafe {settings.version}</p>
              </motion.div>
            </AnimatePresence>
          )}
        </div>
      </div>
    </div>
  );
}
