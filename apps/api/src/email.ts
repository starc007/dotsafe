import type { Env } from "./env";

// The email that delivers a license key: the only copy of it anyone gets,
// since dotsafe issues its own keys and stores only their hashes.

const DOWNLOAD = "https://github.com/starc007/dotsafe/releases/latest/download/dotsafe.dmg";
const DOCS = "https://dotsafe.dev/docs/install";
// A PNG on the site (apps/web/public): most email clients don't show SVG.
const LOGO = "https://dotsafe.dev/email-logo.png";

const escape = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

export function licenseEmail(args: { key: string; name: string | null; limit: number | null; canReply: boolean }) {
  // Only offer replies when a reply-to inbox is set; the sender address may not be one.
  const keep = args.canReply
    ? "Keep this email: it's your proof of purchase. Reply if anything goes wrong."
    : "Keep this email: it's your proof of purchase.";
  const hi = args.name ? `Hi ${args.name.split(" ")[0]},` : "Hi,";
  const macs = args.limit ? `It works on up to ${args.limit} Macs.` : "";
  const text = `${hi}

Thanks for buying dotsafe. Here's your license key:

${args.key}

1. Download dotsafe: ${DOWNLOAD}
2. Open it and paste the key on the first screen. ${macs}

Using the CLI? Run: dotsafe license activate ${args.key}

Install guide: ${DOCS}

${keep}

dotsafe`;

  const html = `<!doctype html>
<html><body style="margin:0;background:#ffffff;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Inter,sans-serif;color:#171717;">
<div style="max-width:520px;margin:0 auto;padding:40px 24px;">
  <table role="presentation" cellpadding="0" cellspacing="0" style="margin:0 0 32px;border-collapse:collapse;"><tr>
    <td style="padding:0 10px 0 0;vertical-align:middle;"><img src="${LOGO}" width="32" height="32" alt="" style="display:block;border:0;border-radius:8px;"></td>
    <td style="vertical-align:middle;font-size:17px;font-weight:600;letter-spacing:-0.01em;">dotsafe</td>
  </tr></table>
  <p style="margin:0 0 16px;font-size:15px;line-height:1.6;">${escape(hi)}</p>
  <p style="margin:0 0 20px;font-size:15px;line-height:1.6;">Thanks for buying dotsafe. Here's your license key:</p>
  <p style="margin:0 0 28px;padding:16px;border-radius:16px;background:#f5f5f5;font-family:ui-monospace,'SF Mono',Menlo,monospace;font-size:15px;word-break:break-all;">${escape(args.key)}</p>
  <ol style="margin:0 0 24px;padding-left:20px;font-size:15px;line-height:1.7;">
    <li><a href="${DOWNLOAD}" style="color:#3f6212;">Download dotsafe</a></li>
    <li>Open it and paste the key on the first screen. ${escape(macs)}</li>
  </ol>
  <p style="margin:0 0 24px;font-size:14px;line-height:1.6;color:#737373;">Using the CLI? Run <code style="font-family:ui-monospace,'SF Mono',Menlo,monospace;color:#171717;">dotsafe license activate &lt;key&gt;</code>. See the <a href="${DOCS}" style="color:#3f6212;">install guide</a>.</p>
  <p style="margin:0;font-size:14px;line-height:1.6;color:#737373;">${escape(keep)}</p>
</div>
</body></html>`;

  return { subject: "Your dotsafe license key", text, html };
}

/** Sends the license email through Cloudflare Email Service. */
export async function sendLicenseEmail(env: Env, to: string, args: { key: string; name: string | null; limit: number | null }) {
  const { subject, text, html } = licenseEmail({ ...args, canReply: !!env.EMAIL_REPLY_TO });
  await env.EMAIL.send({
    to,
    from: { email: env.EMAIL_FROM, name: "dotsafe" },
    ...(env.EMAIL_REPLY_TO ? { replyTo: env.EMAIL_REPLY_TO } : {}),
    subject,
    text,
    html,
  });
}
