import { useEffect, useRef } from "react";

// Every keyboard shortcut in the app, in one place: the handlers and the
// hints shown in buttons, tooltips and Settings all read from here.

import { isMac } from "./platform";

export { isMac };

export type Shortcut = {
  /** `KeyboardEvent.key`, compared case-insensitively. */
  key: string;
  shift?: boolean;
  label: string;
  group: "General" | "Projects" | "Variables";
};

/** All shortcuts use ⌘ (Ctrl outside macOS), so they work while typing. */
export const SHORTCUTS = {
  quickCopy: { key: "k", label: "Find and copy a variable", group: "General" },
  lock: { key: "l", label: "Lock the vault", group: "General" },
  settings: { key: ",", label: "Open or close settings", group: "General" },
  secrets: { key: "s", shift: true, label: "Open secrets", group: "General" },
  newProject: { key: "n", shift: true, label: "New project", group: "Projects" },
  previousProject: { key: "ArrowUp", label: "Previous project", group: "Projects" },
  nextProject: { key: "ArrowDown", label: "Next project", group: "Projects" },
  previousEnvironment: { key: "[", label: "Previous environment", group: "Projects" },
  nextEnvironment: { key: "]", label: "Next environment", group: "Projects" },
  compareEnvironments: { key: "d", label: "Compare environments", group: "Projects" },
  addVariable: { key: "n", label: "Add a variable", group: "Variables" },
  importVariables: { key: "i", label: "Import from .env", group: "Variables" },
  filterVariables: { key: "f", label: "Filter variables", group: "Variables" },
} satisfies Record<string, Shortcut>;

const KEY_NAMES: Record<string, string> = { ArrowUp: "↑", ArrowDown: "↓", ",": ",", "[": "[", "]": "]" };

/** The keys to show for a shortcut, e.g. ["⌘", "⇧", "N"]. */
export function shortcutKeys(s: Shortcut): string[] {
  const keys = [isMac ? "⌘" : "Ctrl"];
  if (s.shift) keys.push(isMac ? "⇧" : "Shift");
  keys.push(KEY_NAMES[s.key] ?? s.key.toUpperCase());
  return keys;
}

/** For `aria-keyshortcuts`, e.g. "Meta+Shift+N". */
export function ariaShortcut(s: Shortcut): string {
  const parts = [isMac ? "Meta" : "Control"];
  if (s.shift) parts.push("Shift");
  parts.push(s.key.length === 1 ? s.key.toUpperCase() : s.key);
  return parts.join("+");
}

function matches(e: KeyboardEvent, s: Shortcut) {
  const mod = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  return mod && !e.altKey && e.shiftKey === !!s.shift && e.key.toLowerCase() === s.key.toLowerCase();
}

/** A dialog or menu is open, so app-level shortcuts should stand aside. */
function overlayOpen() {
  return !!document.querySelector(
    '[role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"], [role="menu"][data-state="open"]',
  );
}

/** Runs `handler` when the shortcut is pressed, unless a dialog is open. */
export function useShortcut(shortcut: Shortcut, handler: () => void, enabled = true) {
  const latest = useRef(handler);
  latest.current = handler;
  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.repeat || !matches(e, shortcut) || overlayOpen()) return;
      e.preventDefault();
      latest.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [shortcut, enabled]);
}
