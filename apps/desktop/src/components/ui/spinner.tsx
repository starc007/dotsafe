import { cn } from "cn"
import { Loader2Icon } from "lucide-react"

/** Spins an HTML wrapper, not the svg: WebKit repaints a rotating svg every
 * frame and snaps it to whole pixels, so the arc wobbles as it turns. A
 * wrapper with will-change gets its own layer, rotated smoothly on the GPU. */
function Spinner({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      data-slot="spinner"
      role="status"
      aria-label="Loading"
      className={cn("inline-flex size-4 shrink-0 animate-spin will-change-transform", className)}
      {...props}
    >
      <Loader2Icon aria-hidden className="block size-full" />
    </span>
  )
}

export { Spinner }
