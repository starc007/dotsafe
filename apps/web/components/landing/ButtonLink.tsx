import Link from 'next/link';
import type { ComponentProps } from 'react';
import { buttonClass, type ButtonVariant } from '@/lib/button';

/** A link styled as a pill button: `primary` for the one main action. */
export function ButtonLink({
  variant = 'outline',
  className,
  ...props
}: ComponentProps<typeof Link> & { variant?: ButtonVariant }) {
  return <Link className={buttonClass(variant, className)} {...props} />;
}
