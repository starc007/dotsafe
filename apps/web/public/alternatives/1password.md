# 1Password for developers alternative: dotsafe vs 1Password

> 1Password stores developer secrets alongside your passwords, in its cloud. dotsafe is a dedicated, open source secret manager whose vault stays on your Mac.

1Password's developer tools, the `op` CLI, Environments and service accounts, keep your project secrets in your 1Password account, encrypted so 1Password can't read them. If your team already lives in 1Password, that's convenient.

dotsafe is built only for developer secrets: projects, environments, rotation and AI agents. Your vault lives on your Mac, synced through storage you already own, with no account or subscription.

## At a glance

| | dotsafe | 1Password |
| --- | --- | --- |
| Where secrets live | Encrypted on your Mac, synced through iCloud, Dropbox, S3 or WebDAV | Your 1Password cloud account |
| Encryption | End to end | End to end (dual-key model) |
| Account needed | No | Yes, plus the desktop app for Environments |
| Source code | Open source, MIT | Proprietary |
| Price | $49 once, up to 2 computers (Mac or Linux) | Subscription: Individual $3.99 a month, Business $8.99 a user per month, billed yearly |
| AI agents | Agents run approved commands and never get values; output is redacted | Environments MCP server shows names only and never returns values |
| Local dev | `dotsafe run -- npm run dev` | `op run --environment <id> -- npm run dev` |
| Teams | One person per license, no team sharing yet | Shared vaults, admin policies, service accounts |

## Where 1Password is the better choice

- **One tool for everything.** Passwords, SSH keys and secrets in one app, on Mac, Windows and Linux.
- **Teams.** Shared vaults and Environments with admin policies and usage reports.
- **CI and cloud.** Service accounts, integrations for GitHub Actions, Jenkins, CircleCI, Kubernetes and Terraform, and a sync to AWS Secrets Manager.

## Where dotsafe fits better

- **Open source and local-first.** Read every line, and keep your vault in storage you control.
- **Made for developer secrets.** Inheritance between environments, `${{ NAME }}` references, shared secrets linked across projects, and one-step rotation through providers.
- **Agents, end to end.** Approval with Touch ID per command, redacted output, a local proxy for OpenAI and Anthropic keys, and an encrypted audit log.
- **Pay once**, with no subscription.

## Switching from 1Password

dotsafe imports an item's fields straight from the 1Password CLI, by label:

```bash
dotsafe import --from 1password --item "my-api prod"
```

Then replace `op run --` with `dotsafe run --`. See [Import and export](/docs/import-export).

## Sources

Checked on November 5, 2026.

- [Business pricing](https://1password.com/pricing/business) and [personal pricing](https://1password.com/pricing/password-manager)
- [Environments](https://www.1password.dev/environments) and [Environments MCP server](https://www.1password.dev/environments/mcp-server)
- [Load secrets into the environment](https://www.1password.dev/cli/secrets-environment-variables)
- [Service accounts](https://www.1password.dev/service-accounts) and [security](https://1password.com/security)
