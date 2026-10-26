/** Keeps the `dark` class on <html> in step with the system appearance, so
 * shadcn's `dark:` styles follow macOS light/dark mode live. */
export function followSystemTheme() {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => document.documentElement.classList.toggle("dark", query.matches);
  apply();
  query.addEventListener("change", apply);
}
