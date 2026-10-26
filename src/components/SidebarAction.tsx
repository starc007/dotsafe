import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { ariaShortcut, type Shortcut } from "@/lib/shortcuts";
import { ShortcutKeys } from "@/components/ShortcutKeys";

/** A sidebar footer row: icon, label, and its keyboard shortcut on the right. */
export function SidebarAction({
  icon,
  label,
  shortcut,
  active,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  shortcut: Shortcut;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      aria-keyshortcuts={ariaShortcut(shortcut)}
      className={cn(
        "group/action flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-muted-foreground outline-none transition-colors",
        "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring",
        "aria-[current=page]:bg-sidebar-accent aria-[current=page]:text-sidebar-accent-foreground [&_svg]:size-4 [&_svg]:shrink-0",
      )}
    >
      {icon}
      <span className="flex-1 truncate">{label}</span>
      <ShortcutKeys
        shortcut={shortcut}
        className="opacity-0 transition-opacity group-hover/action:opacity-100 group-focus-visible/action:opacity-100 group-aria-[current=page]/action:opacity-100"
      />
    </button>
  );
}
