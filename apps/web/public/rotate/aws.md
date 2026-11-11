# Rotate your AWS access keys

> How to rotate an IAM user's access keys without downtime, what to do when one is exposed, and how dotsafe creates the new key, makes it live everywhere and deletes the old one.

An IAM user can have two access keys at once. That's what makes rotation safe: create the second key, move everything to it, then switch off and delete the first.

## At a glance

| | |
| --- | --- |
| Key format | An access key ID and a secret access key. IDs starting `AKIA` are long-term keys; `ASIA` are temporary credentials |
| Environment variables | `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` |
| Keys per user | Two at most |
| Old key after a new one is made | Keeps working until you deactivate or delete it |
| Permissions to rotate | `iam:CreateAccessKey`, `iam:UpdateAccessKey`, `iam:DeleteAccessKey`, `iam:GetAccessKeyLastUsed` and `iam:ListAccessKeys` on the user |
| Exposed keys | AWS may attach the `AWSCompromisedKeyQuarantineV3` policy to the user. Don't remove it |

The secret access key is shown only when the key is created. If it's lost, delete the key and make a new one.

AWS's first advice is to need fewer of these keys: people should sign in with IAM Identity Center, and workloads should use IAM roles and temporary credentials. Long-term access keys are for what can't.

## Rotate a key by hand

AWS's documented process with the CLI:

1. Create a second key while the first is active: `aws iam create-access-key --user-name <user>`.
2. Update every application and tool to the new key.
3. Check the old key is unused: `aws iam get-access-key-last-used --access-key-id <old id>`. AWS suggests waiting several days.
4. Deactivate the old key rather than deleting it: `aws iam update-access-key --user-name <user> --access-key-id <old id> --status Inactive`.
5. Make sure everything still works. If something breaks, set the old key back to `Active`.
6. After another wait, delete it: `aws iam delete-access-key --user-name <user> --access-key-id <old id>`.

Give each application its own key, so one leak can be revoked without breaking the others.

## If the key was exposed

AWS's steps:

1. **Find what it can reach**: the user's policies and groups.
2. **Invalidate it**: create a replacement and delete the exposed key.
3. **Invalidate temporary credentials made from it.** Deleting the key doesn't end sessions it already started; deleting the user, or a deny policy for credentials issued before now, does.
4. **Restore the access you need**, with the new key or better, a role.
5. **Review the account** in CloudTrail for anything the key did.

If AWS has attached the quarantine policy, follow the support case it opens, and contact AWS Support if you're unsure.

## Rotate with dotsafe

dotsafe calls IAM itself, with an admin user's key you connect once. Each rotation creates a new access key for the user, saves it to your vault first, checks it, makes it live in every environment that uses it and pushes it to your deploy targets. `retire` deletes the old key.

```bash
dotsafe secret share AWS_SECRET_ACCESS_KEY --all    # if they're still plain variables
dotsafe secret share AWS_ACCESS_KEY_ID --all
dotsafe connect aws_iam --set access_key_id=AKIA…    # the admin user's ID; asks for its secret
dotsafe secret provider AWS_SECRET_ACCESS_KEY --connection "AWS IAM access key" \
  --set user_name=deploy-bot --set access_key_id_secret=AWS_ACCESS_KEY_ID --key-id AKIA…
```

`access_key_id_secret` names the secret that holds `AWS_ACCESS_KEY_ID`, so the new key ID goes live together with the new secret. `--key-id` is the ID of the key you have now, so dotsafe can delete it after the first rotation. Then:

```bash
dotsafe secret rotate AWS_SECRET_ACCESS_KEY
dotsafe secret retire AWS_SECRET_ACCESS_KEY    # deletes the old key in IAM
```

Retire before the next rotation: the user can't hold more than two keys. If a key was exposed, `--leaked` deletes the old key straight away. dotsafe only deletes keys; for the temporary-credential and CloudTrail steps above, follow AWS's guidance.

See [Integrations](/docs/integrations) and [Secrets and rotation](/docs/secrets).

## Sources

Checked on November 11, 2026.

- [Manage access keys for IAM users](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html) (two keys per user, prefer temporary credentials)
- [Update access keys](https://docs.aws.amazon.com/IAM/latest/UserGuide/id-credentials-access-keys-update.html) (rotation steps)
- [Permissions required to manage access keys](https://docs.aws.amazon.com/IAM/latest/UserGuide/access-keys_required-permissions.html)
- [Secure access keys](https://docs.aws.amazon.com/IAM/latest/UserGuide/securing_access-keys.html) and [programmatic access](https://docs.aws.amazon.com/IAM/latest/UserGuide/security-creds-programmatic-access.html) (`AKIA` and `ASIA`)
- [What to do if you inadvertently expose an AWS access key](https://aws.amazon.com/blogs/security/what-to-do-if-you-inadvertently-expose-an-aws-access-key/)
- [AWSCompromisedKeyQuarantineV3](https://docs.aws.amazon.com/aws-managed-policy/latest/reference/AWSCompromisedKeyQuarantineV3.html)
- [IAM best practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html)
- [AWS CLI environment variables](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-envvars.html)
