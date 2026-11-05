# Threat model

**Status:** proposed. Applies to format v2 and the components in [README.md](README.md).

## Assets

| Asset | Where it lives |
|---|---|
| Secret values | Encrypted records in storage; plaintext only in memory of an unlocked client, and in processes it launches |
| Secret names, project and environment names | Inside encrypted records (never in object names) |
| Vault key | Sealed in key slots; in memory while unlocked |
| Master password, recovery key | The user's head and paper; never stored |
| Device key | OS keychain, optionally gated by Touch ID |
| Storage credentials (S3/R2 keys) | OS keychain |
| Provider tokens (for rotation and push) | Encrypted in the vault, like any other secret |
| Audit log | Encrypted records in the vault |

## Adversaries and outcomes

| Adversary | Can | Can't | Notes |
|---|---|---|---|
| **Storage provider** (Apple, Google, AWS, Cloudflare), or anyone who steals the bucket | Read ciphertext, see object count, sizes and timestamps; delete objects; serve old copies | Read names or values; change records undetected; serve a rolled-back vault undetected (v2) | Padding hides exact sizes. Deletion is denial of service; replicas mitigate it |
| **Network attacker** | Observe traffic to storage and the dotsafe API | Read anything: storage traffic is ciphertext over TLS, the API never sees secrets | |
| **dotsafe (us), or a compromised dotsafe API** | Refuse or revoke license checks | Read or change any vault. Licensing never blocks access to the vault | Being offline is never held against the user |
| **Thief with a locked or powered-off device** | Copy the encrypted cache and synced files | Unlock without the password or the device key. The device key is in the keychain, behind the OS login and (optionally) Touch ID | Remote revoke + re-key protects future changes |
| **Lost device, revoked by the user** | Keep what it already decrypted | Decrypt anything written after the re-key | Secrets it saw must be rotated; dotsafe guides this ([devices-and-keys.md](devices-and-keys.md)) |
| **Prompt-injected or malicious AI agent** using dotsafe's MCP tools | See names and metadata; ask to run commands | Receive plaintext from dotsafe; run anything the user didn't approve; read proxied API keys at all | An approved command still holds its secrets in its environment ([agents.md](agents.md#limits)) |
| **A process on a server running `dotsafe daemon`** (a compromised or new cron job, a script running as the same user) | Reach the daemon's socket; ask to run commands; run the jobs it was already approved for | Approve anything: `dotsafe approve` answers carry an HMAC under a subkey of the vault key, which only the master password yields. Get values from the socket. Change an approved job's script, arguments or keys without a new approval | Root, or the same user with `ptrace`, can read the key from the daemon's memory. With the daemon stopped (after a reboot), nothing on disk unlocks the vault ([agents.md](agents.md#unattended-servers)) |
| **Another user on the same machine** | Nothing beyond normal OS isolation | Read the keychain, cache (encrypted) or broker socket (owner-only permissions, peer check) | |
| **Malware running as the user while the vault is unlocked** | Read process memory, keylog the password, drive the UI | | **Out of scope.** No local secret manager can defend against this. Auto-lock and short grants limit the window |
| **Supply chain** (compromised dependency or build) | Anything | | Mitigated by few dependencies in core, pinned lockfiles, CI, signed releases, and a future third-party audit |

## Security properties

1. **Confidentiality:** names and values are encrypted with XChaCha20-Poly1305 under keys derived from the vault key.
2. **Integrity:** every record and key slot is authenticated, and bound by AAD to the vault and its own id.
3. **Freshness (v2):** a signed vault head with a monotonic counter lets clients detect a rolled-back vault ([storage.md](storage.md#rollback-detection)).
4. **Forward protection after revocation:** re-keying means a revoked device can't read changes made afterwards.
5. **Agent isolation:** agents never receive plaintext from dotsafe; proxied keys never enter the agent's processes.
6. **Availability:** works offline; replicas protect against a provider deleting or losing data.

## Non-goals

- Defending against malware or root on an unlocked device.
- Hiding that a user has a dotsafe vault, or how many records it has.
- Server-side access control or audit logs. There is no server to enforce them.
- Protecting a secret once it's been injected into a process the user approved. That process can do what it likes with it.

## What we may claim

- "Your secrets are encrypted on your device. We never see them, and neither does your storage provider."
- "Agents can only run what you approve." And, for proxied API keys: "Agents use the key without ever seeing it."

What we must not claim: "agents can never read secrets" in general, "unhackable", or anything implying protection against malware on an unlocked device.
