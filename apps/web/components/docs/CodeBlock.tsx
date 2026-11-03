'use client';

import { useEffect, useRef, useState } from 'react';
import { Check, Copy } from 'lucide-react';

/** Shell commands on a dark panel, with a copy button. `dotsafe` is lime and
 * `# comments` are dimmed; the comments are copied too, since they're valid
 * shell. */
export function CodeBlock({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard blocked: the text is still selectable.
    }
  }

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
          onClick={copy}
          aria-label={copied ? 'Copied' : 'Copy'}
          title={copied ? 'Copied' : 'Copy'}
          className="flex size-8 cursor-pointer items-center justify-center rounded-full text-ds-code-muted transition-colors duration-200 hover:bg-ds-code-fg/10 hover:text-ds-code-fg"
        >
          {copied ? <Check aria-hidden className="size-4 text-ds-primary" /> : <Copy aria-hidden className="size-4" />}
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
