import { useState } from 'react';
import { LoaderCircle } from 'lucide-react';
import { apiUrl } from '@/lib/site';
import { buttonClass } from '@/lib/button';
import { cn } from '@/lib/cn';
import { track } from '@/lib/analytics';
import { PLAN } from '@/lib/pricing';

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
      // A checkout exists; payment itself is confirmed by the API's webhook, not here.
      track('checkout_started', { price: PLAN.price, path: window.location.pathname });
      window.location.assign(body.checkout_url);
    } catch (e) {
      setError(e instanceof Error && e.message !== 'Failed to fetch' ? e.message : FALLBACK);
      setBusy(false);
    }
  }

  return (
    <>
      <button
        type="button"
        onClick={buy}
        disabled={busy}
        aria-busy={busy}
        className={buttonClass('primary', cn('relative', className))}
      >
        {/* While checkout starts, the spinner covers the label, which stays in
            place (hidden) so the button keeps its width. */}
        <span className={cn(busy && 'invisible')}>{label}</span>
        {busy && <LoaderCircle aria-hidden className="absolute animate-spin" />}
      </button>
      {error && (
        <p role="alert" className="mt-2 text-center text-xs text-white">
          {error}
        </p>
      )}
    </>
  );
}
