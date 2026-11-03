import Link from 'next/link';
import type { ReactNode } from 'react';
import Markdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { remarkCallouts } from '@/lib/remark-callouts';
import { slugify } from '@/lib/slugify';
import { CodeBlock } from '@/components/docs/CodeBlock';
import { DocsCallout } from '@/components/docs/DocsCallout';

/** The plain text of a heading, for its anchor id. */
function text(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(text).join('');
  if (node && typeof node === 'object' && 'props' in node) return text((node.props as { children?: ReactNode }).children);
  return '';
}

const components: Components = {
  a: ({ href = '', children }) =>
    href.startsWith('/') && !href.endsWith('.md') && !href.endsWith('.txt') ? (
      <Link href={href}>{children}</Link>
    ) : (
      <a href={href}>{children}</a>
    ),
  h2: ({ children }) => {
    const id = slugify(text(children));
    return (
      <h2 id={id} className="group">
        <a href={`#${id}`} className="docs-anchor">
          {children}
        </a>
      </h2>
    );
  },
  h3: ({ children }) => <h3 id={slugify(text(children))}>{children}</h3>,
  pre: ({ node }) => {
    const code = node?.children[0];
    const value = code?.type === 'element' && code.children[0]?.type === 'text' ? code.children[0].value : '';
    return <CodeBlock code={value.replace(/\n$/, '')} />;
  },
  blockquote: ({ children, ...props }) => (
    <DocsCallout kind={(props as { 'data-callout'?: string })['data-callout']}>{children}</DocsCallout>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto">
      <table>{children}</table>
    </div>
  ),
};

/** A docs page body, rendered from its Markdown at build time. */
export function DocsMarkdown({ children }: { children: string }) {
  return (
    <Markdown remarkPlugins={[remarkGfm, remarkCallouts]} components={components}>
      {children}
    </Markdown>
  );
}
