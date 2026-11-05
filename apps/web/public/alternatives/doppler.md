# Doppler alternative: dotsafe vs Doppler

> Doppler is a hosted secrets service priced per user. dotsafe is an open source, local-first alternative: your secrets stay encrypted on your Mac, for one payment.

Doppler keeps your secrets in its cloud and syncs them to your apps, CI and cloud platforms. It's a strong choice for teams. But your secrets live on Doppler's servers, encrypted with keys Doppler's own service uses to decrypt them, and every seat beyond the free tier is a monthly bill.

dotsafe keeps your secrets encrypted on your Mac, end to end, and syncs them through storage you already own. There's no dotsafe server holding them.

## At a glance

| | dotsafe | Doppler |
| --- | --- | --- |
| Where secrets live | Encrypted on your Mac, synced through iCloud, Dropbox, S3 or WebDAV | Doppler's cloud |
| Encryption | End to end: only your Macs can decrypt | On Doppler's servers, with per-workplace keys wrapped by cloud KMS |
| Account needed | No | Yes |
| Source code | Open source, MIT | CLI and MCP server are Apache-2.0; the service is closed source |
| Price | $49 once, up to 2 Macs | Free for 3 users, then $8 a user; Team plan $21 a user per month |
| AI agents | Agents run approved commands and never get values; output is redacted | MCP server (experimental) whose tools can read secret values |
| Works offline | Yes | From an encrypted snapshot of recently fetched secrets |
| Local dev | `dotsafe run -- npm run dev` | `doppler run -- npm run dev` |
| Teams | One person per license, no team sharing yet | Roles, change requests, SSO, SCIM and activity logs |

## Where Doppler is the better choice

- **Teams.** Role-based access per environment, change requests, SAML SSO and SCIM.
- **Syncs everywhere.** Pushes secrets to many platforms, with a Kubernetes operator and CI guides.
- **Audit.** Activity logs, with forwarding to Splunk and Datadog.
- **Platforms.** Works on Linux and Windows as well as macOS.

## Where dotsafe fits better

- **Your secrets never sit on someone else's server.** Not even encrypted with a key they hold.
- **Built for AI agents.** An agent sees variable names, asks to run a command, and you approve it with Touch ID. Values never reach it, and OpenAI and Anthropic keys go through a local proxy.
- **Pay once.** No per-seat subscription for your own projects.

## Switching from Doppler

dotsafe imports straight from the Doppler CLI, for the project and config set up in your directory:

```bash
dotsafe import --from doppler
```

Then replace `doppler run --` with `dotsafe run --` in your scripts. See [Import and export](/docs/import-export).

## Sources

Checked on November 5, 2026.

- [Doppler pricing](https://www.doppler.com/pricing)
- [Security fact sheet](https://docs.doppler.com/docs/security-fact-sheet)
- [CLI guide](https://docs.doppler.com/docs/cli) and [fallback files](https://docs.doppler.com/docs/automatic-fallbacks)
- [Doppler MCP server](https://docs.doppler.com/docs/mcp)
