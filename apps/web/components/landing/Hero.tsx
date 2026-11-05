'use client';

import { motion } from 'motion/react';
import { ArrowRight, Download } from 'lucide-react';
import { links } from '@/lib/site';
import { fadeUp } from '@/lib/motion';
import { ButtonLink } from '@/components/landing/ButtonLink';
import { NotYcBadge } from '@/components/landing/NotYcBadge';
import { ProductShot } from '@/components/landing/ProductShot';

/** Plain white headline; the colour lives in the frame around the product below. */
export function Hero() {
  return (
    <section>
      <motion.div
        className="mx-auto max-w-6xl px-4 pt-32 sm:px-6 sm:pt-40"
        initial="hidden"
        animate="shown"
        transition={{ staggerChildren: 0.08 }}
      >
        <motion.div variants={fadeUp} className="mb-6">
          <NotYcBadge />
        </motion.div>
        <motion.h1
          variants={fadeUp}
          className="max-w-2xl text-[28px] leading-[1.2] font-medium tracking-[-0.02em] text-balance sm:text-[36px]"
        >
          Secrets that never leave your Mac.{' '}
          <span className="text-ds-muted">
            The open source secret manager for developers and their AI agents, synced through storage you already own.
          </span>
        </motion.h1>
        <motion.div variants={fadeUp} className="mt-8 flex flex-wrap items-center gap-2.5">
          <ButtonLink href={links.download} variant="primary">
            Download for macOS
            <Download aria-hidden />
          </ButtonLink>
          <ButtonLink href={links.docs} variant="outline">
            Read the docs
            <ArrowRight aria-hidden />
          </ButtonLink>
          <ButtonLink href={links.linux} variant="ghost" className="px-3">
            Also on Linux
          </ButtonLink>
        </motion.div>
      </motion.div>
      <ProductShot />
    </section>
  );
}
