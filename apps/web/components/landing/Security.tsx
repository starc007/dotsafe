import { EncryptDemo } from '@/components/landing/EncryptDemo';
import { Reveal } from '@/components/landing/Reveal';
import { SectionHeading } from '@/components/landing/SectionHeading';

const STEPS = [
  { title: 'Sealed on your Mac', body: 'You save a value. dotsafe encrypts it with your key before it touches the disk.' },
  { title: 'Stored as noise', body: 'Your folder, bucket or server holds only encrypted files. So does anyone who copies them.' },
  { title: 'Opened on your devices', body: 'Your other Macs decrypt it with the same key, and Touch ID unlocks it day to day.' },
];

export function Security() {
  return (
    <section id="security" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-20 sm:px-6 sm:py-28">
      <SectionHeading eyebrow="Security" title="Encrypted before it leaves your Mac.">
        Your storage only ever holds noise, and there&apos;s no dotsafe server to break into.
      </SectionHeading>
      <Reveal className="mt-10 rounded-2xl border border-ds-line bg-ds-bg p-4 sm:p-8">
        <EncryptDemo />
      </Reveal>
      <ol className="mt-10 grid gap-8 sm:grid-cols-3">
        {STEPS.map((step, i) => (
          <li key={step.title}>
            <Reveal delay={i * 0.08} className="border-t border-ds-line pt-5">
              <p className="font-mono text-xs text-ds-muted tabular-nums">0{i + 1}</p>
              <p className="mt-2 text-[15px] font-medium">{step.title}</p>
              <p className="mt-1.5 text-sm text-pretty text-ds-muted">{step.body}</p>
            </Reveal>
          </li>
        ))}
      </ol>
    </section>
  );
}
