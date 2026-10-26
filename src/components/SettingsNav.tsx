import { useEffect, useRef, type KeyboardEvent } from "react";
import { motion } from "motion/react";
import { ChevronLeft } from "lucide-react";
import { snappy } from "@/lib/motion";
import { SETTINGS_SECTIONS, type SettingsSection } from "@/lib/settingsSections";
import { cn } from "@/lib/utils";
import { Kbd } from "@/components/ui/kbd";

/** The sidebar while Settings is open: a way back, then the sections.
 * Arrow keys move between sections; the list is a single Tab stop. */
export function SettingsNav({
  section,
  onSelect,
  onBack,
}: {
  section: SettingsSection;
  onSelect: (section: SettingsSection) => void;
  onBack: () => void;
}) {
  const list = useRef<HTMLElement>(null);

  // Start keyboard focus on the current section, so arrows work right away.
  useEffect(() => {
    list.current?.querySelector<HTMLButtonElement>('[aria-current="page"]')?.focus();
  }, []);

  function onKeyDown(e: KeyboardEvent) {
    const at = SETTINGS_SECTIONS.findIndex((s) => s.id === section);
    const last = SETTINGS_SECTIONS.length - 1;
    const next =
      e.key === "ArrowDown" ? (at + 1) % SETTINGS_SECTIONS.length
      : e.key === "ArrowUp" ? (at - 1 + SETTINGS_SECTIONS.length) % SETTINGS_SECTIONS.length
      : e.key === "Home" ? 0
      : e.key === "End" ? last
      : null;
    if (next === null || e.metaKey || e.ctrlKey) return;
    e.preventDefault();
    onSelect(SETTINGS_SECTIONS[next].id);
    requestAnimationFrame(() => list.current?.querySelector<HTMLButtonElement>('[aria-current="page"]')?.focus());
  }

  return (
    <>
      <div className="px-2 pb-2">
        <button
          type="button"
          onClick={onBack}
          aria-keyshortcuts="Escape"
          className="group/back flex h-8 w-full items-center gap-1.5 rounded-md px-1.5 text-left text-muted-foreground outline-none transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring"
        >
          <ChevronLeft aria-hidden className="size-4" />
          <span className="flex-1">Back to projects</span>
          <Kbd aria-hidden className="opacity-0 transition-opacity group-hover/back:opacity-100 group-focus-visible/back:opacity-100">
            Esc
          </Kbd>
        </button>
      </div>
      <h2 className="px-4 pb-1 text-xs font-medium text-muted-foreground">Settings</h2>
      <nav ref={list} aria-label="Settings" onKeyDown={onKeyDown} className="flex-1 overflow-y-auto px-2 pb-2">
        {SETTINGS_SECTIONS.map(({ id, label, icon: Icon }) => {
          const current = id === section;
          return (
            <button
              key={id}
              type="button"
              aria-current={current ? "page" : undefined}
              // Roving focus: only the current section is in the Tab order.
              tabIndex={current ? 0 : -1}
              onClick={() => onSelect(id)}
              className={cn(
                "relative flex h-8 w-full items-center gap-2 rounded-md px-2 text-left outline-none transition-colors",
                "text-sidebar-foreground/80 hover:bg-sidebar-accent/60 focus-visible:ring-2 focus-visible:ring-sidebar-ring",
                "aria-[current=page]:font-medium aria-[current=page]:text-sidebar-accent-foreground",
              )}
            >
              {current && (
                <motion.span
                  layoutId="settings-highlight"
                  aria-hidden
                  className="absolute inset-0 rounded-md bg-sidebar-accent"
                  transition={snappy}
                />
              )}
              <Icon aria-hidden className="relative size-4 shrink-0 text-muted-foreground" />
              <span className="relative truncate">{label}</span>
            </button>
          );
        })}
      </nav>
    </>
  );
}
