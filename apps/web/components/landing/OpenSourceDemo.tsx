'use client';

import { motion } from 'motion/react';
import { Code, Fingerprint, KeyRound, Lock } from 'lucide-react';
import { EASE } from '@/lib/motion';

const FACTS = [
  { icon: Lock, label: 'XChaCha20-Poly1305', detail: 'Every record sealed on your Mac' },
  { icon: KeyRound, label: 'Argon2id', detail: 'Your password never leaves it' },
  { icon: Fingerprint, label: 'Touch ID', detail: 'Unlocks and approves, day to day' },
  { icon: Code, label: 'Open source', detail: 'The app, the CLI and the format' },
];

/** The primitives behind the claim, arriving one after another. */
export function OpenSourceDemo() {
  return (
    <motion.ul
      className="grid w-full max-w-md gap-2.5 sm:grid-cols-2"
      initial="hidden"
      whileInView="shown"
      viewport={{ once: true, margin: '-60px' }}
      transition={{ staggerChildren: 0.08 }}
    >
      {FACTS.map(({ icon: Icon, label, detail }) => (
        <motion.li
          key={label}
          variants={{ hidden: { opacity: 0, y: 8 }, shown: { opacity: 1, y: 0, transition: { duration: 0.5, ease: EASE } } }}
          className="rounded-xl border border-ds-line bg-white p-3.5"
        >
          <span className="flex size-8 items-center justify-center rounded-full border border-ds-line">
            <Icon aria-hidden className="size-4" />
          </span>
          <p className="mt-3 text-sm font-medium">{label}</p>
          <p className="text-xs text-ds-muted">{detail}</p>
        </motion.li>
      ))}
    </motion.ul>
  );
}
