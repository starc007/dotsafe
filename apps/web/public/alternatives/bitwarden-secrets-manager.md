# Bitwarden Secrets Manager alternative: dotsafe vs Bitwarden

> Bitwarden Secrets Manager is an end-to-end encrypted cloud service for teams and machines. dotsafe keeps your secrets on your Mac, with no account.

Bitwarden Secrets Manager stores secrets in Bitwarden's cloud (or your own server on Enterprise), encrypted on the client so Bitwarden can't read them. It's built for teams sharing secrets with CI, servers and machine accounts.

dotsafe is end-to-end encrypted too, but there's no account and no service in the middle: your vault lives on your Mac and syncs through storage you already own.

## At a glance

| | dotsafe | Bitwarden Secrets Manager |
| --- | --- | --- |
| Where secrets live | Encrypted on your Mac, synced through iCloud, Dropbox, S3 or WebDAV | Bitwarden's cloud, or self-hosted on Enterprise |
| Encryption | End to end | End to end (zero-knowledge) |
| Account needed | No | Yes |
| Source code | Open source, MIT | Server AGPL 3.0; organization features and the `bws` CLI under Bitwarden's own licenses |
| Price | $49 once, up to 2 Macs | Free for 2 users; Teams $6 and Enterprise $12 per user per month, billed yearly |
| AI agents | Agents run commands you approve with Touch ID and never get values; output is redacted | Machine accounts scope which secrets an agent's process can use |
| Local dev | `dotsafe run -- npm run dev` | `bws run -- 'npm run dev'` |
| Teams | One person per license, no team sharing yet | Users, groups, SSO and SCIM, event logs |

## Where Bitwarden is the better choice

- **Teams.** Share secrets with access control, groups, SSO and SCIM.
- **CI and servers.** Machine accounts, GitHub Actions and GitLab integrations, and a Kubernetes operator.
- **Audit at scale.** Event logs you can export to a SIEM.

## Where dotsafe fits better

- **No account, no cloud.** Your vault never leaves storage you control, and dotsafe works offline.
- **Approval per command.** An agent's request shows the exact command and variables, and runs only after Touch ID.
- **Keys out of the process.** OpenAI and Anthropic keys go through a local proxy, so the command never holds them.
- **Pay once.** No per-user subscription.

## Switching from Bitwarden

Export a project's secrets as `.env` lines with `bws`, import them, and delete the file:

```bash
bws secret list <project-id> --output json | jq -r '.[] | "\(.key)=\(.value)"' > .env
dotsafe import
rm .env
```

## Sources

Checked on November 5, 2026.

- [Bitwarden Secrets Manager](https://bitwarden.com/products/secrets-manager/) and [business pricing](https://bitwarden.com/pricing/business/)
- [Secrets Manager overview](https://bitwarden.com/help/secrets-manager-overview/) and [CLI](https://bitwarden.com/help/secrets-manager-cli/)
- [Bitwarden license FAQ](https://github.com/bitwarden/server/blob/main/LICENSE_FAQ.md) and [SDK license](https://github.com/bitwarden/sdk-sm)
