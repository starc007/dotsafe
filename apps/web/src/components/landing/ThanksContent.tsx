import { useEffect, useRef, useState } from 'react';
import { Check, Copy, Download } from 'lucide-react';
import { links } from '@/lib/site';
import { track } from '@/lib/analytics';
import { buttonClass } from '@/lib/button';

type Return = { status: string | null; licenseKey: string | null; email: string | null };

declare global {
  /** What Dodo sent back on the return URL, read (and wiped from the address
   * bar) by the inline script on /thanks before anything else runs. */
  interface Window {
    __dotsafeReturn?: Return;
  }
}

/** The page after checkout. The status is only a hint for what to say; the
 * key itself comes by email too. */
export function ThanksContent() {
  const [ret, setRet] = useState<Return | null>(null);
  useEffect(() => setRet(window.__dotsafeReturn ?? { status: null, licenseKey: null, email: null }), []);

  // Dodo's status is a hint, not proof of payment (the API's webhook records
  // real purchases), so this is only the return. Never the key or the email.
  const reported = useRef(false);
  useEffect(() => {
    if (!ret || reported.current) return;
    reported.current = true;
    track('checkout_returned', { status: ret.status ?? 'unknown' });
  }, [ret]);
  if (!ret) return <div className="min-h-[50vh]" />;

  if (ret.status === 'failed' || ret.status === 'expired' || ret.status === 'cancelled') {
    return (
      <Heading
        title={ret.status === 'expired' ? 'Checkout expired.' : "Payment didn't go through."}
        rest="You weren't charged. Try again whenever you're ready."
      >
        <a href="/pricing" className={buttonClass('primary')}>
          Back to pricing
        </a>
      </Heading>
    );
  }

  const paid = ret.status === 'succeeded' || ret.status === 'active';
  return (
    <>
      <Heading
        title={paid ? 'Thanks for buying dotsafe.' : 'Your payment is on its way.'}
        rest={
          ret.email
            ? `Your license key is on its way to ${ret.email}.`
            : paid
              ? 'Your license key is on its way to your email.'
              : "We'll email your license key as soon as it clears."
        }
      >
        <a href={links.download} className={buttonClass('primary')}>
          Download for macOS
          <Download aria-hidden />
        </a>
        <a href={links.docs} className={buttonClass('outline')}>
          Read the docs
        </a>
        <a href={links.linux} className={buttonClass('ghost', 'px-3')}>
          On Linux?
        </a>
      </Heading>
      <div className="fade-up mt-12 max-w-xl rounded-3xl border border-ds-line p-6 text-sm sm:p-8">
        {ret.licenseKey && <LicenseKey value={ret.licenseKey.split(',')[0]} />}
        <ol className="flex flex-col gap-3">
          <Step n={1}>Download dotsafe and open it.</Step>
          <Step n={2}>Paste your license key on the first screen. It works on up to 2 computers.</Step>
          <Step n={3}>
            Using the CLI instead? Run <code className="font-mono text-[13px]">dotsafe license activate &lt;key&gt;</code>
          </Step>
        </ol>
      </div>
    </>
  );
}

function Heading({ title, rest, children }: { title: string; rest: string; children: React.ReactNode }) {
  return (
    <>
      <h1 className="max-w-2xl text-[28px] leading-[1.2] font-medium tracking-[-0.02em] text-balance sm:text-[36px]">
        {title} <span className="text-ds-muted">{rest}</span>
      </h1>
      <div className="mt-8 flex flex-wrap items-center gap-2.5">{children}</div>
    </>
  );
}

function LicenseKey({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    await navigator.clipboard.writeText(value);
    track('license_key_copied');
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }
  return (
    <div className="mb-6">
      <p className="mb-2 text-ds-muted">Your license key</p>
      <div className="flex items-center gap-2 rounded-full bg-ds-shell py-1 pr-1 pl-4">
        <span className="min-w-0 flex-1 truncate font-mono text-[13px] select-all">{value}</span>
        <button
          type="button"
          onClick={copy}
          aria-label={copied ? 'Copied' : 'Copy license key'}
          title={copied ? 'Copied' : 'Copy license key'}
          className="flex size-8 shrink-0 items-center justify-center rounded-full text-ds-muted transition-colors hover:bg-ds-card hover:text-ds-fg [&_svg]:size-4"
        >
          {copied ? <Check aria-hidden className="text-ds-accent" /> : <Copy aria-hidden />}
        </button>
      </div>
    </div>
  );
}

function Step({ n, children }: { n: number; children: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="flex size-5 shrink-0 items-center justify-center rounded-full bg-ds-shell text-xs font-medium tabular-nums">
        {n}
      </span>
      <span>{children}</span>
    </li>
  );
}
