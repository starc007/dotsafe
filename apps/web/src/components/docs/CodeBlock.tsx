import { Check, Copy } from 'lucide-react';

/** Shell commands on a dark panel, with a copy button. `dotsafe` is lime and
 * `# comments` are dimmed; the comments are copied too, since they're valid
 * shell. Rendered at build time; the button works through
 * src/scripts/copy-code.ts, which Markdown.astro loads. */
export function CodeBlock({ code }: { code: string }) {
  return (
    <div className="my-6 overflow-hidden rounded-2xl bg-ds-code text-ds-code-fg">
      <div className="flex h-9 items-center justify-between pr-1 pl-4">
        <span className="flex items-center gap-1.5" aria-hidden>
          <span className="size-2 rounded-full bg-ds-code-fg/15" />
          <span className="size-2 rounded-full bg-ds-code-fg/15" />
          <span className="size-2 rounded-full bg-ds-code-fg/15" />
        </span>
        <button
          type="button"
          data-copy-code={code}
          aria-label="Copy"
          title="Copy"
          className="group flex size-8 cursor-pointer items-center justify-center rounded-full text-ds-code-muted transition-colors duration-200 hover:bg-ds-code-fg/10 hover:text-ds-code-fg"
        >
          <Copy aria-hidden className="size-4 group-data-copied:hidden" />
          <Check aria-hidden className="hidden size-4 text-ds-primary group-data-copied:block" />
        </button>
      </div>
      <pre className="overflow-x-auto px-4 pb-4 font-mono text-[13px] leading-relaxed">
        <code>
          {code.split('\n').map((line, i) => (
            <span key={i} className="block">
              <Line text={line} />
            </span>
          ))}
        </code>
      </pre>
    </div>
  );
}

/** Colours one line: a leading `dotsafe` in lime, and a trailing `# comment`
 * (not a `#` inside quotes) dimmed. */
function Line({ text }: { text: string }) {
  let quote: string | null = null;
  let cut = text.length;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === "'" || c === '"') {
      quote = c;
    } else if (c === '#' && (i === 0 || text[i - 1] === ' ')) {
      cut = i;
      break;
    }
  }
  const command = text.slice(0, cut);
  const comment = text.slice(cut);
  const lead = command.match(/^(\s*)(dotsafe)(?=\s|$)/);
  return (
    <>
      {lead ? (
        <>
          {lead[1]}
          <span className="text-ds-primary">{lead[2]}</span>
          {command.slice(lead[0].length)}
        </>
      ) : (
        command
      )}
      {comment && <span className="text-ds-code-muted">{comment}</span>}
      {!text && ' '}
    </>
  );
}
