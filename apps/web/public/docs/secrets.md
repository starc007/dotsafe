# Secrets and rotation

> Share one value across environments, rotate it in one step and keep its history.

A plain variable belongs to one environment. A **secret** is a value that many environments, in any project, can link to. Rotate it once and every linked environment gets the new value. Secrets also keep their history, so a bad rotation can be undone.

## Share a variable

```bash
dotsafe secret share STRIPE_KEY            # turn this variable into a secret
dotsafe secret share STRIPE_KEY --all      # and link every environment that had the same value
dotsafe secret link STRIPE_KEY STRIPE_KEY -e production   # link one more variable to it
dotsafe secret unlink STRIPE_KEY -e production            # give it its own copy again
dotsafe secret list                        # age and where each one is used
```

Copies of a secret's value that aren't linked are flagged in the app, so they don't go stale after the next rotation.

## Rotate

```bash
dotsafe secret rotate STRIPE_KEY                     # type the new value
dotsafe secret rotate JWT_SECRET --generate          # or have dotsafe make one
dotsafe secret rotate STRIPE_KEY --note "yearly"     # kept in the history
```

The new value goes live everywhere at once. The old one is kept as the *previous* value until you retire it, so you have time to revoke it with the provider without downtime.

```bash
dotsafe secret retire STRIPE_KEY        # once the old value is revoked
```

### If a key leaked

```bash
dotsafe secret rotate STRIPE_KEY --leaked
```

`--leaked` retires the old value right away instead of keeping it as previous. If the secret has a [provider](/docs/integrations), the old key is revoked at the provider too.

## History, rollback and restore

```bash
dotsafe secret history STRIPE_KEY       # versions and notes, never values
dotsafe secret rollback STRIPE_KEY      # back to the previous value
dotsafe secret restore STRIPE_KEY 3     # make version 3 current again
```

## Rotation policies

Give a secret a rotation interval and dotsafe tells you when it's due.

```bash
dotsafe secret policy STRIPE_KEY --every 90   # days; 0 removes the policy
dotsafe secret due                            # what's due now
```

The app sends a notification when a secret comes due, and marks it in the sidebar.

## After removing a device

When you [remove a device](/docs/devices), every value it could read goes into a rotation queue. It stays in the sidebar until each one is rotated.

```bash
dotsafe secret queue              # what's left
dotsafe secret queue --rotate     # rotate every queued secret that has a provider
```
