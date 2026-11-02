import { FaqItem } from '@/components/landing/FaqItem';
import { Reveal } from '@/components/landing/Reveal';
import { SectionHeading } from '@/components/landing/SectionHeading';

const QUESTIONS = [
  {
    q: 'Where are my secrets stored?',
    a: 'In a vault in storage you pick: iCloud Drive, Google Drive, Dropbox, an S3 or R2 bucket, or a WebDAV server. Everything is encrypted on your Mac before it gets there.',
  },
  {
    q: 'Can dotsafe see my secrets?',
    a: "No. There's no dotsafe server holding your vault. The only server we run handles licenses, and it sees your license key and your Mac's name, nothing else.",
  },
  {
    q: 'What if I forget my password?',
    a: "You get a recovery key when you create a vault. It's the only way back in, so keep it somewhere safe. We can't reset it for you.",
  },
  {
    q: 'Do AI agents see my keys?',
    a: 'No. Claude Code, Cursor and other agents can run commands with your secrets only after you approve them with Touch ID, and the values are taken out of anything they read back.',
  },
  {
    q: 'Does it work offline?',
    a: "Yes. Your vault lives on your Mac. If the license can't be checked for 30 days, the app asks you to go online. It never locks you out.",
  },
  {
    q: 'Is there a free version?',
    a: "No. dotsafe is $49, paid once, for up to 2 Macs. You enter your license key the first time you open the app or the CLI.",
  },
  {
    q: 'Can I move my license to another Mac?',
    a: 'Yes. Remove it in Settings on the old Mac to free the slot, then activate it on the new one.',
  },
  {
    q: 'Can I bring my existing .env files?',
    a: 'Yes. Drop them into the app, or import from Doppler, Infisical and 1Password with the CLI.',
  },
  {
    q: 'Is it open source?',
    a: 'Yes. The app, the CLI and the vault format are all on GitHub, so you can read every line.',
  },
];

/** Common questions, each opening in place. */
export function Faq() {
  return (
    <section id="faq" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-20 sm:px-6 sm:py-28">
      <div className="grid gap-10 md:grid-cols-5">
        <div className="md:col-span-2">
          <SectionHeading eyebrow="FAQ" title="Questions, answered.">
            Anything else, ask on GitHub.
          </SectionHeading>
        </div>
        <Reveal className="md:col-span-3">
          <ul className="border-t border-ds-line">
            {QUESTIONS.map(({ q, a }) => (
              <FaqItem key={q} q={q} a={a} />
            ))}
          </ul>
        </Reveal>
      </div>
    </section>
  );
}
