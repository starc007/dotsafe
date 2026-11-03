'use client';

import { useEffect, useRef, useState } from 'react';
import { Check, Copy } from 'lucide-react';

/** Shell commands with a copy button, `# comments` dimmed. The comments are
 * copied too: they're valid shell. */
export function CodeBlock({ children }: { children: string }) {
  const code = children.replace(/^\n+|\s+$/g, '');
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
    <div className="group relative my-5 rounded-2xl bg-ds-shell">
      <pre className="overflow-x-auto p-4 pr-12 font-mono text-[13px] leading-relaxed">
        <code>
          {code.split('\n').map((line, i) => (
            <span key={i} className="block">
              <Line text={line} />
            </span>
          ))}
        </code>
      </pre>
      <button
        type="button"
        onClick={copy}
        aria-label={copied ? 'Copied' : 'Copy'}
        title={copied ? 'Copied' : 'Copy'}
        className="absolute top-2.5 right-2.5 flex size-8 cursor-pointer items-center justify-center rounded-full text-ds-muted transition-[color,background-color,opacity] duration-200 hover:bg-ds-card hover:text-ds-fg focus-visible:opacity-100 sm:opacity-0 sm:group-hover:opacity-100"
      >
        {copied ? <Check aria-hidden className="size-4 text-ds-accent" /> : <Copy aria-hidden className="size-4" />}
      </button>
    </div>
  );
}

/** Dims a trailing `# comment`, but not a `#` inside quotes. */
function Line({ text }: { text: string }) {
  let quote: string | null = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === "'" || c === '"') {
      quote = c;
    } else if (c === '#' && (i === 0 || text[i - 1] === ' ')) {
      return (
        <>
          {text.slice(0, i)}
          <span className="text-ds-muted">{text.slice(i)}</span>
        </>
      );
    }
  }
  return <>{text || ' '}</>;
}
