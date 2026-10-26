import { useSyncExternalStore } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "dotsafe:theme";
const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
const listeners = new Set<() => void>();
let preference: ThemePreference = readPreference();

function readPreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "light" || stored === "dark" ? stored : "system";
  } catch {
    return "system";
  }
}

function resolved(): ResolvedTheme {
  if (preference !== "system") return preference;
  return systemDark.matches ? "dark" : "light";
}

/** Puts `.dark` on <html> for shadcn's `dark:` styles, and matches the native
 * window (title bar, traffic lights) to the chosen theme. */
function apply() {
  const theme = resolved();
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.documentElement.style.colorScheme = theme;
  // Best effort: theming must never stop the app from rendering, and
  // getCurrentWindow() throws outside a real Tauri window (e.g. the browser mock).
  try {
    getCurrentWindow()
      .setTheme(preference === "system" ? null : preference)
      .catch(() => {});
  } catch {
    // Not in a Tauri window.
  }
  listeners.forEach((notify) => notify());
}

/** Call once at startup. Re-applies when the system appearance changes. */
export function initTheme() {
  apply();
  systemDark.addEventListener("change", apply);
}

export function setThemePreference(next: ThemePreference) {
  preference = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Storage unavailable: the choice still applies for this session.
  }
  apply();
}

function subscribe(notify: () => void) {
  listeners.add(notify);
  return () => listeners.delete(notify);
}

export function useTheme() {
  // The snapshot is a string so React can compare it cheaply.
  const snapshot = useSyncExternalStore(subscribe, () => `${preference}:${resolved()}`);
  const [pref, theme] = snapshot.split(":") as [ThemePreference, ResolvedTheme];
  return { preference: pref, theme, setPreference: setThemePreference };
}
