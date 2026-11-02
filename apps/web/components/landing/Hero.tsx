'use client';

import { motion } from 'motion/react';
import { Download } from 'lucide-react';
import { links } from '@/lib/site';
import { fadeUp } from '@/lib/motion';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { ProductShot } from '@/components/landing/ProductShot';
import { ShaderBackdrop } from '@/components/landing/ShaderBackdrop';

export function Hero() {
  return (
    <section className="relative overflow-hidden">
      {/* The gradient sweeps in from the right; white keeps the headline side clean. */}
      <div className="absolute inset-x-0 top-0 h-[720px]">
        <ShaderBackdrop className="opacity-80" />
        <div className="absolute inset-0 bg-[linear-gradient(90deg,#fff_38%,rgb(255_255_255/0.6)_60%,rgb(255_255_255/0)_85%)] max-md:bg-[linear-gradient(180deg,#fff_40%,rgb(255_255_255/0.65)_70%,rgb(255_255_255/0.25))]" />
        <div className="absolute inset-x-0 bottom-0 h-48 bg-gradient-to-b from-white/0 to-white" />
      </div>
      <motion.div
        className="relative mx-auto max-w-6xl px-4 pt-28 sm:px-6 sm:pt-32"
        initial="hidden"
        animate="shown"
        transition={{ staggerChildren: 0.08 }}
      >
        <motion.h1
          variants={fadeUp}
          className="max-w-xl text-[26px] leading-[1.25] font-medium tracking-[-0.0125em] text-balance sm:text-[30px]"
        >
          Secrets that never leave your Mac.{' '}
          <span className="text-ds-muted">Encrypted on your device, synced through storage you already own.</span>
        </motion.h1>
        <motion.div variants={fadeUp} className="mt-7 flex flex-wrap items-center gap-2.5">
          <ButtonLink href={links.download} variant="primary">
            <Download aria-hidden />
            Download for Mac
          </ButtonLink>
          <ButtonLink href={links.docs} variant="outline">
            Read the docs
          </ButtonLink>
        </motion.div>
        <motion.p variants={fadeUp} className="mt-4 text-xs text-ds-muted">
          Free to try · macOS 13 or later · Open source
        </motion.p>
      </motion.div>
      <ProductShot />
    </section>
  );
}
