import { useEffect, useState } from "react";
import { ChevronRight, CircleCheck, GitCompareArrows, Plus } from "lucide-react";
import { api, message, type EnvironmentDiff, type EnvironmentView } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { SurfaceDialog } from "@/components/SurfaceDialog";

/** The environment to compare with first: the next one in deploy order, or
 * the one before it for the last (production → staging). */
export function defaultOther(environments: EnvironmentView[], current: EnvironmentView) {
  const at = environments.findIndex((e) => e.id === current.id);
  return environments[at + 1] ?? environments[at - 1] ?? null;
}

/** Compares one environment with another in the same project, by name.
 * Values are compared in Rust and never reach the webview. */
export function CompareDialog({
  open,
  onOpenChange,
  environment,
  environments,
  otherId,
  onOtherChange,
  onAdd,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  environment: EnvironmentView;
  environments: EnvironmentView[];
  otherId: string;
  onOtherChange: (id: string) => void;
  /** Add a variable missing from one side, by name; the value is typed fresh. */
  onAdd: (environment: EnvironmentView, key: string) => void;
}) {
  const others = environments.filter((e) => e.id !== environment.id);
  const other = others.find((e) => e.id === otherId) ?? others[0];
  const [result, setResult] = useState<{ environmentId: string; diff: EnvironmentDiff } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showSame, setShowSame] = useState(false);

  // The last result stays on screen while the next loads (after switching
  // the other environment, or coming back from adding a variable), so the
  // list doesn't flash a skeleton. Only a different first environment does.
  useEffect(() => {
    if (!open || !other) return;
    let live = true;
    setError(null);
    api
      .compareEnvironments(environment.id, other.id)
      .then((diff) => live && setResult({ environmentId: environment.id, diff }))
      .catch((e) => live && setError(message(e)));
    return () => {
      live = false;
    };
  }, [open, environment.id, other?.id]);

  if (!other) return null;
  const diff = result?.environmentId === environment.id ? result.diff : null;
  const identical = diff && !diff.onlyInFirst.length && !diff.onlyInSecond.length && !diff.different.length;

  return (
    <SurfaceDialog
      open={open}
      onOpenChange={onOpenChange}
      icon={<GitCompareArrows aria-hidden />}
      title="Compare environments"
      description="Names only. Values stay hidden."
      className="sm:max-w-lg"
    >
      <div className="flex flex-col gap-4">
        <div className="flex min-w-0 items-center gap-2 text-sm">
          <span className="shrink-0 font-medium">{environment.name}</span>
          <span className="shrink-0 text-muted-foreground">with</span>
          <Tabs value={other.id} onValueChange={onOtherChange} className="min-w-0">
            <TabsList aria-label="Compare with" className="bg-muted p-0.5">
              {others.map((e) => (
                <TabsTrigger key={e.id} value={e.id} className="px-3 py-1 text-[13px]">
                  {e.name}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
        </div>

        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : !diff ? (
          <div aria-hidden className="flex flex-col gap-2">
            <Skeleton className="h-3.5 w-32" />
            <Skeleton className="h-3.5 w-48" />
            <Skeleton className="h-3.5 w-40" />
          </div>
        ) : (
          <div className="flex max-h-[50vh] flex-col gap-4 overflow-y-auto">
            {identical && (
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <CircleCheck aria-hidden className="size-4 text-primary-ink" />
                Same variables and values.
              </p>
            )}
            <Group
              title={`Only in ${environment.name}`}
              keys={diff.onlyInFirst}
              addTo={other}
              onAdd={onAdd}
            />
            <Group
              title={`Only in ${other.name}`}
              keys={diff.onlyInSecond}
              addTo={environment}
              onAdd={onAdd}
            />
            <Group title="Different values" keys={diff.different} />
            {diff.same.length > 0 && (
              <section className="flex flex-col gap-1">
                <button
                  type="button"
                  aria-expanded={showSame}
                  onClick={() => setShowSame((s) => !s)}
                  className="flex items-center gap-1 self-start rounded-full text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  <ChevronRight aria-hidden className={cn("size-3.5", showSame && "rotate-90")} />
                  Same in both ({diff.same.length})
                </button>
                {showSame && <Rows keys={diff.same} />}
              </section>
            )}
          </div>
        )}
      </div>
    </SurfaceDialog>
  );
}

function Group({
  title,
  keys,
  addTo,
  onAdd,
}: {
  title: string;
  keys: string[];
  addTo?: EnvironmentView;
  onAdd?: (environment: EnvironmentView, key: string) => void;
}) {
  if (keys.length === 0) return null;
  return (
    <section className="flex flex-col gap-1">
      <h3 className="text-xs font-medium text-muted-foreground">
        {title} ({keys.length})
      </h3>
      <Rows keys={keys} addTo={addTo} onAdd={onAdd} />
    </section>
  );
}

function Rows({
  keys,
  addTo,
  onAdd,
}: {
  keys: string[];
  addTo?: EnvironmentView;
  onAdd?: (environment: EnvironmentView, key: string) => void;
}) {
  return (
    <ul className="divide-y">
      {keys.map((key) => (
        <li key={key} className="flex h-9 items-center gap-3">
          <span title={key} className="min-w-0 flex-1 truncate font-mono text-[12.5px]">
            {key}
          </span>
          {addTo && onAdd && (
            <Button variant="ghost" size="sm" className="shrink-0" onClick={() => onAdd(addTo, key)}>
              <Plus aria-hidden />
              Add to {addTo.name}
            </Button>
          )}
        </li>
      ))}
    </ul>
  );
}
