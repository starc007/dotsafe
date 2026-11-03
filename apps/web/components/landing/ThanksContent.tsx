'use client';

import { useEffect, useRef, useState } from 'react';
import { Check, Copy, Download } from 'lucide-react';
import { links } from '@/lib/site';
import { track } from '@/lib/analytics';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { Reveal } from '@/components/landing/Reveal';

type Return = { status: string | null; licenseKey: string | null; email: string | null };

/** What Dodo sent back on the return URL. Read once, then the query is wiped
 * from the address bar so the license key doesn't linger in history. The
 * status is only a hint for what to say; the key itself comes by email too. */
function readReturn(): Return {
  const params = new URLSearchParams(window.location.search);
  const value = { status: params.get('status'), licenseKey: params.get('license_key'), email: params.get('email') };
  if (window.location.search) window.history.replaceState(null, '', window.location.pathname);
  return value;
}

export function ThanksContent() {
  const [ret, setRet] = useState<Return | null>(null);
  // Once only: reading wipes the query, so a second run (Strict Mode) would find nothing.
  const read = useRef(false);
  useEffect(() => {
    if (read.current) return;
    read.current = true;
    setRet(readReturn());
  }, []);

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
        <ButtonLink href="/pricing" variant="primary">
          Back to pricing
        </ButtonLink>
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
        <ButtonLink href={links.download} variant="primary">
          Download for macOS
          <Download aria-hidden />
        </ButtonLink>
        <ButtonLink href={links.docs} variant="outline">
          Read the docs
        </ButtonLink>
      </Heading>
      <Reveal className="mt-12 max-w-xl rounded-3xl border border-ds-line p-6 text-sm sm:p-8">
        {ret.licenseKey && <LicenseKey value={ret.licenseKey.split(',')[0]} />}
        <ol className="flex flex-col gap-3">
          <Step n={1}>Download dotsafe and open it.</Step>
          <Step n={2}>Paste your license key on the first screen. It works on up to 2 Macs.</Step>
          <Step n={3}>
            Using the CLI instead? Run <code className="font-mono text-[13px]">dotsafe license activate &lt;key&gt;</code>
          </Step>
        </ol>
      </Reveal>
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
