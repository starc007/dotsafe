/** The blog. Each post is a Markdown file in `public/blog/<slug>.md`, starting
 * with `# Title` and `> description` like the docs, served as is at
 * `/blog/<slug>.md` and rendered into its page. Adding one is a file plus an
 * entry here; the index, sitemap, llms.txt and feed follow. */
export type Post = {
  slug: string;
  /** `YYYY-MM-DD`. The "Updated" date comes from git (lib/updated.ts). */
  published: string;
};

export const BLOG = {
  href: '/blog',
  title: 'Blog',
  description: 'How dotsafe is built, and what we learned keeping secrets away from AI agents.',
};

/** Newest first. */
export const POSTS: Post[] = [
  { slug: 'best-env-file-alternatives-2027', published: '2026-11-13' },
  { slug: 'best-secret-managers-for-ai-coding-agents-2027', published: '2026-11-12' },
  { slug: 'ai-agents-and-env-files', published: '2026-11-12' },
];

export const BLOG_PATHS = [BLOG.href, ...POSTS.map((p) => `${BLOG.href}/${p.slug}`)];
