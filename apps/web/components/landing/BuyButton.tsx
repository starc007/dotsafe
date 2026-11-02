'use client';

import { useState } from 'react';
import { ArrowRight, LoaderCircle } from 'lucide-react';
import { apiUrl } from '@/lib/site';
import { buttonClass } from '@/lib/button';

const FALLBACK = "Couldn't start checkout. Try again in a moment.";

/** Starts a Dodo checkout through the API and sends the visitor to it. Dodo
 * brings them back to /thanks, and emails the license key. */
export function BuyButton({ label, className }: { label: string; className?: string }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function buy() {
    setBusy(true);
    setError(null);
    try {
      const res = await fetch(`${apiUrl}/v1/checkout`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: '{}',
      });
      const body = await res.json().catch(() => null);
      if (!res.ok || typeof body?.checkout_url !== 'string') throw new Error(body?.error?.message ?? FALLBACK);
      window.location.assign(body.checkout_url);
    } catch (e) {
      setError(e instanceof Error && e.message !== 'Failed to fetch' ? e.message : FALLBACK);
      setBusy(false);
    }
  }

  return (
    <>
      <button type="button" onClick={buy} disabled={busy} aria-busy={busy} className={buttonClass('primary', className)}>
        {label}
        {/* The icon swaps, the label stays, so the button keeps its width. */}
        {busy ? <LoaderCircle aria-hidden className="animate-spin" /> : <ArrowRight aria-hidden />}
      </button>
      {error && (
        <p role="alert" className="mt-2 text-center text-xs text-white">
          {error}
        </p>
      )}
    </>
  );
}
