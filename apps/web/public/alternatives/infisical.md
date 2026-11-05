# Infisical alternative: dotsafe vs Infisical

> Infisical is an open source secrets platform for teams, in the cloud or self-hosted. dotsafe is open source too, but there's no server at all: secrets stay encrypted on your Mac.

Infisical is a full platform: secrets, certificates, KMS and privileged access, on Infisical Cloud or your own servers. Its server encrypts your data at rest and decrypts it to answer requests, and its cloud plans are priced per identity, human or machine.

dotsafe is for developers who want none of that to run: secrets are encrypted on your Mac, end to end, and synced through storage you already have.

## At a glance

| | dotsafe | Infisical |
| --- | --- | --- |
| Where secrets live | Encrypted on your Mac, synced through iCloud, Dropbox, S3 or WebDAV | Infisical Cloud, or a server you host |
| Encryption | End to end: only your Macs can decrypt | On the server, which decrypts to answer requests |
| Source code | Open source, MIT | MIT, except enterprise features under a separate license |
| Price | $49 once, up to 2 Macs | Free for 5 identities; Pro $20 an identity per month, billed yearly |
| Something to run | Nothing | Nothing on the cloud plan, or your own server |
| AI agents | Agents run approved commands and never get values; local proxy for OpenAI and Anthropic keys | MCP server returns values unless masked; Agent Vault proxies credentials |
| Local dev | `dotsafe run -- npm run dev` | `infisical sm run -- npm run dev` |
| Teams | One person per license, no team sharing yet | Access controls, SSO, SCIM, approvals and audit logs |

## Where Infisical is the better choice

- **Teams and self-hosting.** An open source server you can run yourself, with access control, approvals and audit.
- **More than secrets.** Dynamic secrets, certificates, KMS and 100+ integrations, plus a Kubernetes operator.
- **Platforms.** Linux and Windows, CI and production.

## Where dotsafe fits better

- **No server, no account.** Nothing to host, sign in to, or keep up to date.
- **End to end encryption.** Your vault can only be read on your Macs.
- **Approval per command.** Agents ask, you approve with Touch ID, and every request is in an encrypted audit log.
- **Pay once**, instead of per identity.

## Switching from Infisical

dotsafe imports straight from the Infisical CLI, for the project linked in your directory:

```bash
dotsafe import --from infisical
```

Then use `dotsafe run --` where you used `infisical sm run --`. See [Import and export](/docs/import-export).

## Sources

Checked on November 5, 2026.

- [Infisical pricing](https://infisical.com/pricing)
- [Security](https://infisical.com/docs/internals/security)
- [CLI reference](https://infisical.com/docs/cli/reference-agents)
- [Agent Vault](https://infisical.com/docs/documentation/platform/agent-vault/overview) and [Infisical MCP server](https://github.com/Infisical/infisical-mcp-server)
- [Infisical license](https://github.com/Infisical/infisical/blob/main/LICENSE)
