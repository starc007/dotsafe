import { AgentDemo } from '@/components/landing/AgentDemo';
import { FeatureCard } from '@/components/landing/FeatureCard';
import { OpenSourceDemo } from '@/components/landing/OpenSourceDemo';
import { RotationDemo } from '@/components/landing/RotationDemo';
import { SectionHeading } from '@/components/landing/SectionHeading';
import { TerminalDemo } from '@/components/landing/TerminalDemo';

export function Features() {
  return (
    <section id="features" className="mx-auto max-w-6xl scroll-mt-20 px-4 py-20 sm:px-6 sm:py-28">
      <SectionHeading eyebrow="Features" title="One vault for every key.">
        In the app and in your terminal, with secrets that rotate in one place and agents that never see them.
      </SectionHeading>
      <div className="mt-10 grid gap-4 md:grid-cols-5">
        <FeatureCard className="md:col-span-3" title="One secret, linked everywhere" demo={<RotationDemo />}>
          Link a key into every environment that uses it. Rotate it once, through OpenAI, Cloudflare or AWS, and every
          project picks up the new value.
        </FeatureCard>
        <FeatureCard className="md:col-span-2" delay={0.08} title="Agents never see the values" demo={<AgentDemo />}>
          Claude Code and Cursor run commands with your secrets after you approve them with Touch ID. What they read back
          has the values taken out.
        </FeatureCard>
        <FeatureCard className="md:col-span-2" title="A CLI for your terminal" demo={<TerminalDemo />}>
          Run anything with your variables, catch keys before you commit, or import from Doppler, Infisical and
          1Password.
        </FeatureCard>
        <FeatureCard className="md:col-span-3" delay={0.08} title="Open source, nothing to trust" demo={<OpenSourceDemo />}>
          Read every line. Your vault is encrypted before it leaves your Mac, and there&apos;s no dotsafe server holding
          it.
        </FeatureCard>
      </div>
    </section>
  );
}
