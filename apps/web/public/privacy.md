# Privacy policy

> What dotsafe collects, why, who handles it, and what never leaves your Mac.

Last updated: November 4, 2026

dotsafe is built so that your secrets never reach us. This page explains the little we do collect: what you give us when you buy a license, what the app sends to check it, and what the website measures.

## What never leaves your Mac

- **Your vault.** Variable names, values, project and environment names are encrypted on your Mac before they're written anywhere. The storage you choose (iCloud Drive, Google Drive, Dropbox, an S3 or R2 bucket, WebDAV) only ever holds encrypted files. We have no copy and no way to read one.
- **Your master password and recovery key.** They're never sent anywhere or stored. We can't reset them for you.
- **Integrations and AI agents.** When dotsafe rotates a key with OpenAI, Cloudflare, AWS, Vercel or another service, your Mac talks to that service directly with your own tokens. Agent requests are handled on your Mac. None of it passes through us.
- **Usage data.** The app and the CLI have no analytics, telemetry or crash reporting.

## What we collect

### When you buy a license

Payments are handled by [Dodo Payments](https://dodopayments.com), who act as the merchant of record. Checkout asks for your email, name and country, plus a ZIP or postal code where tax needs it, and your payment details go straight to Dodo. We never see your card.

From the purchase, we keep:

- your email address and name
- the amount, currency and date of the payment, and the payment's ID at Dodo
- any refund or dispute on it

We use these to issue and email your license key, to help you if you lose it, and to handle refunds.

### Your license key and devices

We store your license key only as a one-way hash, plus its last four characters so we can identify it when you contact us. The email we send you is the only full copy.

When you activate dotsafe on a Mac, and when the app checks the license about twice a day, it sends the key and the activation's ID. On activation it also sends your Mac's name (for example, "Ada's MacBook") so you can tell your devices apart. We keep each activation's device name and when it was activated, last checked and deactivated.

### Emails

We send your license key by email through Cloudflare Email Service. If you write to us at [hi@dotsafe.dev](mailto:hi@dotsafe.dev), we keep the conversation to help you.

### The website

The website uses [Tracwell](https://tracwell.app) to understand how people find and use it. It records the pages you visit, where you came from (the referring site and any campaign tags), your browser, operating system and device type, and an approximate location (country, region and city) worked out from your IP address. It also records a few actions, such as starting a checkout, copying a docs page or opening a download link.

Tracwell keeps an anonymous visitor ID and session ID in your browser's local storage. It sets no cookies, doesn't store your IP address, and doesn't track you across other websites. It respects your browser's Do Not Track setting: with it on, nothing is sent. Your license key and email are never sent to it, even on the page you return to after buying.

### Service logs

Our API runs on Cloudflare. Your IP address is used to limit requests per client, which stops abuse, and Cloudflare keeps standard request logs. Our own logs leave out license keys, email addresses and names.

### Updates and downloads

Downloads and app updates come from [GitHub](https://github.com/starc007/dotsafe/releases). The app checks GitHub for a new version and downloads one only when you click. GitHub sees these requests as it would any download.

## Who handles your data

| Service | What it does for dotsafe |
| --- | --- |
| Dodo Payments | Takes payments, handles tax and invoices, processes refunds |
| Cloudflare | Hosts the website and the license API, stores purchase and license records, sends the license email |
| Tracwell | Website analytics |
| GitHub | Hosts the source code, downloads and app updates |

We don't sell your data or share it with anyone else, and we don't use it for advertising.

## How long we keep it

We keep purchase and license records for as long as your license exists, and longer where tax and accounting law requires. Website analytics are kept for the period Tracwell's plan allows, up to four years.

## Your choices and rights

You can ask us to show you, correct or delete the personal data we hold about you. Email [hi@dotsafe.dev](mailto:hi@dotsafe.dev). Deleting your license data means your key stops working. Dodo keeps its own records of the payment, which it may have to retain by law.

To stop website analytics, turn on Do Not Track in your browser or clear its local storage for dotsafe.dev.

## Children

dotsafe is a tool for developers and isn't directed at children under 16.

## Changes

If this policy changes, we'll update this page and the date at the top. For significant changes, we'll also say so in the [changelog](/changelog).

## Contact

Questions about privacy: [hi@dotsafe.dev](mailto:hi@dotsafe.dev).
