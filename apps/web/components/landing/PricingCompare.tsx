import { Check, Minus } from 'lucide-react';
import { Reveal } from '@/components/landing/Reveal';

// Mirrors the plans in PricingPlans; placeholder until the real plans are set.
const ROWS: { feature: string; free: string | boolean; pro: string | boolean }[] = [
  { feature: 'Macs', free: '1', pro: 'Up to 3' },
  { feature: 'Variables and environments', free: 'Unlimited', pro: 'Unlimited' },
  { feature: 'The dotsafe CLI', free: true, pro: true },
  { feature: 'iCloud, Google Drive, Dropbox, S3, WebDAV', free: true, pro: true },
  { feature: 'Imports from .env, Doppler, Infisical, 1Password', free: true, pro: true },
  { feature: 'Secrets linked across environments', free: false, pro: true },
  { feature: 'Key rotation through providers', free: false, pro: true },
  { feature: 'Agent access with Touch ID', free: false, pro: true },
  { feature: 'Updates', free: 'Always', pro: 'A year, then keep your version' },
];

function Cell({ value }: { value: string | boolean }) {
  if (value === true) return <Check aria-label="Included" className="size-4 text-ds-accent" />;
  if (value === false) return <Minus aria-label="Not included" className="size-4 text-ds-muted/60" />;
  return <span>{value}</span>;
}

export function PricingCompare() {
  return (
    <Reveal className="overflow-x-auto rounded-3xl border border-ds-line">
      <table className="w-full min-w-[520px] text-left text-sm">
        <thead>
          <tr className="border-b border-ds-line">
            <th scope="col" className="px-6 py-4 font-medium text-ds-muted">
              Compare plans
            </th>
            <th scope="col" className="w-36 px-6 py-4 font-medium sm:w-56">
              Free
            </th>
            <th scope="col" className="w-36 px-6 py-4 font-medium sm:w-56">
              Pro
            </th>
          </tr>
        </thead>
        <tbody>
          {ROWS.map((row) => (
            <tr key={row.feature} className="border-b border-ds-line last:border-0">
              <th scope="row" className="px-6 py-3.5 font-normal text-ds-muted">
                {row.feature}
              </th>
              <td className="px-6 py-3.5">
                <Cell value={row.free} />
              </td>
              <td className="px-6 py-3.5">
                <Cell value={row.pro} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Reveal>
  );
}
