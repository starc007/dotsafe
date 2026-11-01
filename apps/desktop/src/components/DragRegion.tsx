import { cn } from "@/lib/utils";

/** Lets the window be dragged from this area (we hide the native title bar). */
export function DragRegion({ className }: { className?: string }) {
  return <div data-tauri-drag-region className={cn(className)} />;
}
