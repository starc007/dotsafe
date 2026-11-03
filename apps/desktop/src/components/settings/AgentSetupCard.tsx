import { useState } from "react";
import { toast } from "sonner";
import { ArrowUpRight, Bot, Check, Copy } from "lucide-react";
import { api, message } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { IconButton } from "@/components/IconButton";
import { ActionSwapIcon } from "@/components/motion/action-swap";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/motion/tabs";
import { Surface, SurfaceBody, SurfaceDescription, SurfaceFooter, SurfaceHeader, SurfaceTitle } from "@/components/Surface";

// Apps opened from the Dock get a PATH without /usr/local/bin, so JSON
// configs name the CLI by its full path. Terminal agents find it on PATH.
const JSON_CONFIG = `{
  "mcpServers": {
    "dotsafe": {
      "command": "/usr/local/bin/dotsafe",
      "args": ["mcp"]
    }
  }
}`;

const AGENTS = [
  {
    id: "claude",
    label: "Claude Code",
    snippet: "claude mcp add dotsafe -- dotsafe mcp",
    hint: "Run this in a terminal.",
  },
  {
    id: "codex",
    label: "Codex",
    // Full path: the Codex app and IDE extension share this config.
    snippet: "codex mcp add dotsafe -- /usr/local/bin/dotsafe mcp",
    hint: "Run this in a terminal. The Codex app and IDE extension pick it up too.",
  },
  {
    id: "json",
    label: "Cursor & others",
    snippet: JSON_CONFIG,
    hint: "Add this to ~/.cursor/mcp.json, or to the MCP settings of Claude Desktop, Windsurf and other apps.",
  },
] as const;

/** How to add dotsafe as an MCP server, per agent, with a link to the guide. */
export function AgentSetupCard() {
  const [agent, setAgent] = useState<string>(AGENTS[0].id);

  async function openGuide() {
    try {
      await api.openDocs("agents");
    } catch (e) {
      toast.error(message(e));
    }
  }

  return (
    <Surface aria-labelledby="agents-setup-title">
      <SurfaceHeader icon={<Bot aria-hidden />}>
        <SurfaceTitle id="agents-setup-title">Connect an agent</SurfaceTitle>
        <SurfaceDescription>Agents see variable names and ask before running anything.</SurfaceDescription>
      </SurfaceHeader>
      <SurfaceBody>
        <Tabs value={agent} onValueChange={setAgent} className="flex flex-col gap-3">
          <TabsList aria-label="Agent" wrapperClassName="self-start" className="bg-muted p-0.5">
            {AGENTS.map((a) => (
              <TabsTrigger key={a.id} value={a.id} className="py-1 text-[13px]">
                {a.label}
              </TabsTrigger>
            ))}
          </TabsList>
          {AGENTS.map((a) => (
            <TabsContent key={a.id} value={a.id} className="flex flex-col gap-2">
              <Snippet text={a.snippet} label={a.label} />
              <p className="text-xs text-muted-foreground">{a.hint}</p>
            </TabsContent>
          ))}
        </Tabs>
        <p className="mt-3 text-xs text-muted-foreground">
          Install the command-line tool below first. Agents can only reach dotsafe while this app is open and unlocked.
        </p>
        <SurfaceFooter>
          <Button variant="secondary" onClick={openGuide}>
            Agents guide
            <ArrowUpRight aria-hidden />
          </Button>
        </SurfaceFooter>
      </SurfaceBody>
    </Surface>
  );
}

/** A setup command or config with a copy button. Not a secret, so the web clipboard is fine. */
function Snippet({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const multiline = text.includes("\n");

  async function copy() {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  const button = (
    <IconButton label={`Copy the ${label} setup`} onClick={copy}>
      <ActionSwapIcon value={copied ? "copied" : "copy"} animation="blur" className="size-4">
        {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
      </ActionSwapIcon>
    </IconButton>
  );

  if (multiline) {
    return (
      <div className="relative rounded-2xl bg-muted/60 py-3 pr-12 pl-3.5">
        <pre className="selectable overflow-x-auto font-mono text-[12.5px] leading-relaxed">{text}</pre>
        <div className="absolute top-1.5 right-1.5">{button}</div>
      </div>
    );
  }
  return (
    <div className="flex items-center gap-1 rounded-full bg-muted/60 py-1 pr-1 pl-3.5">
      <code className="selectable min-w-0 flex-1 truncate font-mono text-[12.5px]">{text}</code>
      {button}
    </div>
  );
}
