# Best secret managers for AI coding agents in 2027

> Seven secret managers compared on what matters when Claude Code, Cursor or Codex need your API keys: whether the agent sees values, who approves what runs, and whether the key ever reaches the process.

Coding agents run your tests, migrations and dev servers, and all of that needs real keys. Most secret managers now ship an MCP server or an agent guide. They make very different choices about what the agent can see.

We make dotsafe, so read this knowing that. Each tool's section says who it suits, and dotsafe isn't the answer for every row. Every claim about another product links to its own docs, checked on the date at the bottom.

## What to look for

Five questions sort these tools quickly:

1. **Can the agent read values?** Through MCP tools, a CLI it can run, or a file it can open. If yes, every key is in its context.
2. **Who approves what runs?** A person per command, a person per session, a scoped token, or nobody.
3. **Does the process hold the key?** Even with approval, a command that holds a key can print it or send it somewhere.
4. **Is there an audit log?** So you can see what an agent asked for.
5. **Does it fit how you work?** Team or solo, hosted or local, what it costs.

## At a glance

| | Agent can read values | Approval | Key kept out of the process | Price |
| --- | --- | --- | --- | --- |
| dotsafe | No: names only | Each command, once or for a while; Touch ID on Mac | Yes, for OpenAI and Anthropic keys | $49 once, 2 computers |
| 1Password | No through its MCP server: names only | Per Environment, until 1Password locks | No | From $3.99 a month; Business $8.99 a user |
| Infisical | Yes by default; a masking option hides them | Through access policies | Yes, with Agent Vault | Free for 5 identities; Pro $20 an identity |
| Doppler | Yes: its MCP server can read values | Scoped tokens | No | Free for 3 users, then $8 a user; Team $21 a user |
| Bitwarden Secrets Manager | The process gets what its machine account can reach | Scoped machine accounts | No | Teams $6, Enterprise $12 a user |
| HashiCorp Vault | Yes: its MCP server can read and write | Vault policies and tokens | No | Community edition you run yourself |
| dotenvx | The process gets real values | Before decrypt, with Armor | No | Free; Armor from $5 a month |

Monthly prices are per month, billed yearly where the vendor bills that way.

## dotsafe

**Best for:** developers who want agents to use keys without ever seeing them, on Mac or Linux.

dotsafe's MCP server has four tools, and none returns a value: agents list projects and variable names, ask to run a command, or create a random secret they never see. Each command is shown to you with its exact arguments, working directory and variables, and runs only after you approve it, with Touch ID on a Mac. An approval covers that exact command, once or for a while, and ends when the vault locks. Output comes back with every secret replaced by `[dotsafe:NAME]`, including encoded copies.

For OpenAI and Anthropic keys it goes a step further: the command gets a placeholder, and a local proxy adds the real key on the way to the provider, so the process never holds it. Every request goes into an encrypted audit log. We wrote up the design, and where it stops working, in [Your AI coding agent shouldn't read your .env](/blog/ai-agents-and-env-files).

**Where it falls short:** one person per license, with no team sharing yet. An approved command still holds keys that aren't behind the proxy. No Windows.

## 1Password

**Best for:** people and teams who already keep secrets in 1Password.

1Password's Environments MCP server runs inside the desktop app and never returns values: agents can list environments and secret names, create environments and add secrets, but can't read what's stored. You approve access per Environment in the app, and the approval lasts until 1Password locks. Commands get secrets through `op run --environment <id>`, and on Mac and Linux it can mount a `.env` file that's never written to disk. Enterprise admins can switch the MCP server off by policy.

**Where it falls short:** approval is per Environment, not per command, and `op run` hands the process real values. It's a subscription, and you need the desktop app.

## Infisical

**Best for:** teams that want a hosted or self-hosted platform with agent controls.

Infisical's MCP server returns secret values by default; set `INFISICAL_MASK_SECRET_VALUES=true` and tools return `<masked>` instead. Access is enforced by the machine identity the server uses, and you can limit which tools it exposes. The stronger option is Agent Vault, which brokers credentials so agents call APIs, MCP servers, databases and SSH without holding the credentials, with access bundles and a log of every request.

**Where it falls short:** values are on by default in the MCP server, so set it up with care. There's a server to run or a service to trust, and paid plans are per identity.

## Doppler

**Best for:** teams that want hosted secrets synced to many platforms.

Doppler's MCP server is marked experimental and can read secret values, download configs and change secrets. A `--read-only` flag removes the write tools, and Doppler says only a scoped token enforces what the server can reach. Commands get secrets with `doppler run`.

**Where it falls short:** no masking or approval step for agents, so whatever the token can reach, the agent can read. Treat the MCP server as a development tool, as Doppler does.

## Bitwarden Secrets Manager

**Best for:** teams already on Bitwarden who want scoped access for machines and agents.

Machine accounts see only the secrets and projects you give them, and Bitwarden pitches them for AI agent access. Commands run with `bws run`. Bitwarden's official MCP server covers the password vault and organization admin, not Secrets Manager.

**Where it falls short:** the process gets every value its machine account can reach, with no approval or masking step. Bitwarden itself warns against running untrusted programs with `bws run`.

## HashiCorp Vault

**Best for:** companies that already run Vault for infrastructure.

Vault's MCP server is in beta, and its tools can read, write and delete secrets in KV mounts, manage mounts and issue PKI certificates. Its docs warn it may expose secrets to MCP clients and LLMs, and it's meant for local use with trusted clients. Access follows your Vault policies and tokens.

**Where it falls short:** it's infrastructure, not a developer tool: a server to run, unseal and upgrade. Nothing stops an agent with a token from reading what the token allows.

## dotenvx

**Best for:** keeping encrypted `.env` files in git, for free.

`dotenvx run` decrypts your file and gives the process the real values. dotenvx has announced agent features for its 3.0 release, including a `Dotenvxfile` that redacts values in output; check its changelog for when they ship. The paid Armor service can require a person to approve each decrypt and keeps an audit log of every access.

**Where it falls short:** without Armor, anything that can run `dotenvx run` gets every value, and secrets live in git history forever, even encrypted.

## How to choose

- **You want agents to use keys without seeing them, and you work alone:** dotsafe.
- **You already pay for 1Password:** its Environments MCP server is a good default.
- **Your team needs a shared platform with an agent credential broker:** Infisical, with masking on and Agent Vault for API keys.
- **You're already on Doppler, Bitwarden or Vault:** use scoped, read-only tokens for agents, and don't give them more than one project.
- **You want free and file-based:** dotenvx, and add Armor if agents run your commands.

Whatever you pick, keep keys out of files the agent can open, approve the narrowest thing that works, and rotate any key an agent has seen. Our [key rotation guides](/rotate) cover OpenAI, Anthropic, Stripe, AWS and Cloudflare.

## Sources

Checked on November 12, 2026.

- 1Password: [Environments MCP server](https://www.1password.dev/environments/mcp-server), [secure AI access](https://www.1password.dev/get-started/secure-ai-access), [personal](https://1password.com/pricing/password-manager) and [business pricing](https://1password.com/pricing/business)
- Infisical: [MCP server](https://github.com/Infisical/infisical-mcp-server), [Agent Vault](https://infisical.com/docs/documentation/platform/agent-vault/overview), [pricing](https://infisical.com/pricing)
- Doppler: [MCP server](https://docs.doppler.com/docs/mcp), [pricing](https://www.doppler.com/pricing)
- Bitwarden: [Secrets Manager CLI](https://bitwarden.com/help/secrets-manager-cli/), [MCP server](https://github.com/bitwarden/mcp-server), [business pricing](https://bitwarden.com/pricing/business/)
- HashiCorp Vault: [MCP server](https://developer.hashicorp.com/vault/docs/mcp-server/overview) and [its tools](https://github.com/hashicorp/vault-mcp-server), [installing Vault](https://developer.hashicorp.com/vault/docs/get-vault)
- dotenvx: [agents](https://dotenvx.com/agents), [Armor](https://dotenvx.com/armor), [pricing](https://dotenvx.com/pricing), [changelog](https://github.com/dotenvx/dotenvx/blob/main/CHANGELOG.md)
- dotsafe: [agent docs](/docs/agents) and [source](https://github.com/starc007/dotsafe)
