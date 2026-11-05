# HashiCorp Vault alternative for developers: dotsafe vs Vault

> Vault is built for running secrets infrastructure across a company. If you need your own app secrets on your Mac, dotsafe is simpler.

HashiCorp Vault (now part of IBM) is a secrets server: you run it, unseal it, write policies, and connect apps to it. It's the right tool for dynamic credentials and access control across many teams and machines. For a developer who wants their API keys out of `.env` files and safely in reach of their AI agents, it's a lot to operate.

dotsafe takes the other approach. There's no server: your secrets are encrypted on your Mac and synced through storage you already have.

## At a glance

| | dotsafe | HashiCorp Vault |
| --- | --- | --- |
| Where secrets live | Encrypted on your Mac, synced through iCloud, Dropbox, S3 or WebDAV | On a Vault server you run, or HCP Vault Dedicated |
| Encryption | End to end: only your Macs can decrypt | On the server, which holds the keys once unsealed |
| Source code | Open source, MIT | Business Source License 1.1 (since 1.15); the OpenBao fork is MPL 2.0 |
| Price | $49 once, up to 2 Macs | Community edition free to self-host; HCP Vault Dedicated billed per cluster hour plus per client |
| Something to run | Nothing | A Vault server, unsealing, storage, upgrades |
| AI agents | MCP server; agents run approved commands and never get values | Official MCP server (beta) whose tools can read and write secret values |
| Local dev | `dotsafe run -- npm run dev` | Vault Agent in process supervisor mode, or `vault kv get` |
| Teams and policies | One person per license, no team sharing yet | Fine-grained policies, identities, audit devices |

## Where Vault is the better choice

- **Dynamic secrets.** Vault creates short-lived database and cloud credentials on demand and revokes them when their lease ends.
- **Access control at scale.** Identity-based policies and audit across many teams, services and machines.
- **Infrastructure.** Runs on Linux, Windows and Kubernetes, with high availability and replication.

If you run a platform team, use Vault (or OpenBao). If you want your own app secrets encrypted and usable by your agents today, use dotsafe.

## A note on HCP Vault Secrets

HCP Vault Secrets, HashiCorp's hosted secrets service for developers, reached end of sale in June 2025 and has since been shut down. If you used it for app secrets, dotsafe covers the same need without a cloud account.

## Switching from Vault

dotsafe has no direct Vault importer. Export a path as `.env` lines, import it, and delete the file:

```bash
vault kv get -format=json secret/my-app \
  | jq -r '.data.data | to_entries[] | "\(.key)=\(.value)"' > .env
dotsafe import
rm .env
```

Then run your app with `dotsafe run -- <command>`, and give your agents access with [dotsafe for Claude Code](/for/claude-code).

## Sources

Checked on November 5, 2026.

- [Vault license](https://github.com/hashicorp/vault/blob/main/LICENSE) and [OpenBao](https://openbao.org)
- [HCP Vault Dedicated tiers and features](https://developer.hashicorp.com/hcp/docs/vault/tiers-and-features) and [IBM HashiCorp pricing](https://www.ibm.com/products/hashicorp/pricing)
- [Seal/Unseal](https://developer.hashicorp.com/vault/docs/concepts/seal)
- [Vault Agent process supervisor mode](https://developer.hashicorp.com/vault/docs/agent-and-proxy/agent/process-supervisor)
- [Vault MCP server](https://developer.hashicorp.com/vault/docs/mcp-server/overview)
- [HCP Vault Secrets end of life](https://support.hashicorp.com/hc/en-us/articles/41802449287955)
