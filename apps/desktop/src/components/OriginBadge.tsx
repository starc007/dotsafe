import { ArrowDownRight, PenLine } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/**
 * Where a variable's value comes from, when inheritance is involved: the
 * parent environment, or this environment's own value replacing the parent's.
 */
export function OriginBadge({ kind, parent, env }: { kind: "inherited" | "own"; parent: string; env: string }) {
  const inherited = kind === "inherited";
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge
          tabIndex={0}
          className={
            inherited
              ? "bg-inherited/12 font-normal text-inherited outline-none focus-visible:ring-2 focus-visible:ring-ring"
              : "bg-overridden/12 font-normal text-overridden outline-none focus-visible:ring-2 focus-visible:ring-ring"
          }
        >
          {inherited ? <ArrowDownRight aria-hidden /> : <PenLine aria-hidden />}
          {inherited ? `From ${parent}` : "Own value"}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>
        {inherited
          ? `Set in ${parent}. Edit it to give ${env} its own value.`
          : `${parent} has this too, but ${env} uses its own value. Delete it to use ${parent}'s.`}
      </TooltipContent>
    </Tooltip>
  );
}
