# Servers and cron jobs

> Run nightly jobs on a Linux server with dotsafe secrets, without the master password or a token stored on the machine.

A server that runs jobs on a schedule usually keeps its keys in a `.env` file or a token for a secret manager. Either one sits on the disk, in backups and in snapshots. With dotsafe, the server keeps nothing usable on disk: you unlock once after a reboot, the vault key stays in memory, and each job you approve gets only the keys you approved.

The server needs the same vault as your other devices: an [S3 or R2 bucket, WebDAV, or a synced folder](/docs/storage).

## Set it up

After each reboot, SSH in and start the daemon. It asks for the master password once, then keeps running after you log out:

```bash
dotsafe daemon --detach
```

In your crontab, run each job through `dotsafe run`:

```bash
0 3 * * * cd /srv/backup && dotsafe run --only S3_KEY,S3_SECRET -- ./backup.sh
```

With no password and no `dotsafe unlock` session on the machine, `dotsafe run` hands the command to the daemon. The daemon starts it with the keys in its environment, on the job's own output, so cron logs and mails it as usual. The exit code comes back unchanged.

## Approve a job once

The first time a job asks for keys, it waits for approval (5 minutes by default, `--wait 30s` to change it). Approve it from any SSH session on the server:

```bash
dotsafe approve
```

It shows the command, the folder, the script and the keys it wants, and asks for the master password. Choose:

- **once**: this run only;
- **an hour**;
- **until the daemon stops**: the job runs every night without asking, until the server reboots.

If nobody answers in time, the job fails with a message saying so, and the request stays open for a day. Approve it in the morning and the next night's run goes through.

Easier still: run the job by hand once after setting it up, and approve it from a second SSH window.

```bash
dotsafe approve --list                 # what's waiting
dotsafe approve 3f2a --for stop        # answer one by its id
dotsafe approve 3f2a --deny
```

## What asks again

An approval covers exactly what you saw:

- the same program (by its full path) and the same arguments;
- the same folder and environment;
- the same keys: a job that asks for one more asks again;
- the same script: if `backup.sh` (or `python job.py`'s `job.py`) changes, the next run asks again.

Approvals live only in the daemon's memory. Stopping the daemon or rebooting clears them all.

## What this protects, and what it doesn't

- **Nothing usable on disk.** No password, no token, no key file. A copied disk, backup or snapshot can't read your secrets, and after a reboot nothing works until you unlock again.
- **No new jobs without you.** A script you didn't approve, a changed script, or an approved job asking for more keys has to wait for `dotsafe approve`, which needs the master password. A process on the server can reach the daemon, but it can't approve anything.
- **Root can still read memory.** While the daemon runs, root (or anything running as your user that can debug processes) can read the vault key from its memory. This protects the disk and keeps jobs to what you approved; it doesn't protect against someone who already controls the running server.
- **A job holds what it was given.** An approved job has its keys in its environment and could leak them. Approve only the keys each job needs, with `--only`.
- **Its output isn't scrubbed.** Unlike [agent runs](/docs/agents), a job's output goes straight to its log. Don't print secrets.
- **Every run is logged.** `dotsafe audit` shows each run, allowed or denied, with names only.

> [!NOTE]
> Some Linux setups stop a user's processes at logout. If the daemon is gone after you log out, run `loginctl enable-linger` once.

`dotsafe run` refuses the same commands agents can't run, such as `env`, `printenv` and `sh -c`, since those would print every key. Put the job in a script instead.

The daemon also serves [AI agents](/docs/agents) on the server, with the same approvals.
