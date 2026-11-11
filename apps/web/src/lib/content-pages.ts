/** The alternatives and use-case pages, in the order the hubs list them.
 * Each is a Markdown file in `public/<section>/<slug>.md`, served as is for
 * agents and rendered into its page. Adding one is a file plus an item here;
 * the hub, sitemap and llms.txt follow. `label` is the short name on the hub's
 * card and `blurb` its one line (the page's own title and description are
 * longer, for search). `logo` is the product's mark in `public/brands/`
 * (see its README); without one, the card shows the label's initials. */
export type ContentItem = { slug: string; label: string; blurb: string; logo?: string };

export const ALTERNATIVES = {
  href: '/alternatives',
  title: 'dotsafe alternatives and comparisons',
  description:
    'How dotsafe compares with Doppler, Infisical, 1Password, HashiCorp Vault, dotenvx, Bitwarden Secrets Manager and SOPS.',
  groups: [
    {
      title: 'Compare',
      items: [
        { slug: 'doppler', label: 'Doppler', blurb: 'Cloud secrets, priced per user. dotsafe keeps them on your Mac.', logo: 'doppler' },
        { slug: 'infisical', label: 'Infisical', blurb: 'A platform you host or rent. dotsafe has no server at all.', logo: 'infisical' },
        { slug: '1password', label: '1Password', blurb: 'Secrets beside your passwords, in the cloud. dotsafe is local-first.', logo: '1password' },
        { slug: 'hashicorp-vault', label: 'HashiCorp Vault', blurb: 'Secrets infrastructure for companies. dotsafe is for your own apps.', logo: 'hashicorp-vault' },
        { slug: 'dotenvx', label: 'dotenvx', blurb: 'Encrypted .env files in git. dotsafe keeps secrets out of the repo.', logo: 'dotenvx' },
        { slug: 'bitwarden-secrets-manager', label: 'Bitwarden Secrets Manager', blurb: 'An encrypted cloud for teams. dotsafe needs no account.', logo: 'bitwarden' },
        { slug: 'sops', label: 'SOPS', blurb: 'Encrypted config files in git. dotsafe keeps secrets out of the repo.' },
      ] as ContentItem[],
    },
  ],
};

export const USE_CASES = {
  href: '/for',
  title: 'dotsafe for your stack',
  description: 'Use dotsafe with AI coding agents, your framework, and the platforms you deploy to.',
  groups: [
    {
      title: 'AI agents',
      items: [
        { slug: 'claude-code', label: 'Claude Code', blurb: 'Run tests and dev servers with real keys it never sees.', logo: 'claude' },
        { slug: 'cursor', label: 'Cursor', blurb: 'Let the agent use your keys to run commands, never to read them.', logo: 'cursor' },
        { slug: 'codex', label: 'Codex', blurb: 'Codex tasks with real API keys, kept out of what it reads.', logo: 'codex' },
      ] as ContentItem[],
    },
    {
      title: 'Frameworks',
      items: [
        { slug: 'nextjs', label: 'Next.js', blurb: 'Replace .env.local, and keep secrets out of NEXT_PUBLIC_.', logo: 'nextjs' },
        { slug: 'vite', label: 'Vite', blurb: 'VITE_ variables and server secrets, without .env files.', logo: 'vite' },
        { slug: 'nodejs', label: 'Node.js', blurb: 'Drop dotenv: process.env from an encrypted vault.', logo: 'nodejs' },
        { slug: 'python', label: 'Python', blurb: 'Django, Flask and FastAPI secrets into os.environ.', logo: 'python' },
        { slug: 'rails', label: 'Rails', blurb: 'ENV and RAILS_MASTER_KEY, encrypted on your Mac.', logo: 'rails' },
      ] as ContentItem[],
    },
    {
      title: 'Platforms',
      items: [
        { slug: 'vercel', label: 'Vercel', blurb: 'Push each rotated secret to your Vercel projects.', logo: 'vercel' },
        { slug: 'cloudflare-workers', label: 'Cloudflare Workers', blurb: 'Set Worker secrets on every rotation, no wrangler secret put.', logo: 'cloudflare-workers' },
      ] as ContentItem[],
    },
  ],
};

/** How to rotate each provider's keys, by hand and with dotsafe. The
 * providers are the ones dotsafe rotates through (`SERVICES` in
 * crates/integrations); keep the two in step. */
export const ROTATION = {
  href: '/rotate',
  title: 'Rotate an API key',
  description:
    'How to rotate OpenAI, Anthropic, Stripe, AWS and Cloudflare keys without downtime, what to do when one leaks, and how dotsafe does it in one step.',
  groups: [
    {
      title: 'Rotated by dotsafe',
      items: [
        { slug: 'openai', label: 'OpenAI', blurb: 'New project key, live everywhere, old one deleted when you say.', logo: 'openai' },
        { slug: 'cloudflare', label: 'Cloudflare', blurb: 'Roll an API token and push the new value to your Workers.', logo: 'cloudflare' },
        { slug: 'aws', label: 'AWS access keys', blurb: 'Create, switch and delete IAM access keys in one step.', logo: 'aws' },
      ] as ContentItem[],
    },
    {
      title: 'Guided by dotsafe',
      items: [
        { slug: 'anthropic', label: 'Anthropic', blurb: 'Make the key in the Console; dotsafe does the rest.', logo: 'anthropic' },
        { slug: 'stripe', label: 'Stripe', blurb: 'Rotate in the Dashboard; dotsafe pushes the new key everywhere.', logo: 'stripe' },
      ] as ContentItem[],
    },
  ],
};

export type Section = typeof ALTERNATIVES | typeof USE_CASES | typeof ROTATION;

/** Every page of a section, as `/section/slug`. */
export function sectionPaths(section: Section) {
  return section.groups.flatMap((g) => g.items.map((item) => `${section.href}/${item.slug}`));
}

export const CONTENT_PATHS = [ALTERNATIVES, USE_CASES, ROTATION].flatMap((s) => [s.href, ...sectionPaths(s)]);
