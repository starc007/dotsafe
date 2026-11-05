/** A brand's mark from public/brands as a one-colour mask: every logo,
 * gradients included, takes the text colour, so a set of them reads as one.
 * Decorative: the name is always right beside it. */
export function BrandMark({ name, className = 'size-[18px]' }: { name: string; className?: string }) {
  const mask = `url(/brands/${name}.svg) center / contain no-repeat`;
  return <span aria-hidden className={`${className} bg-current`} style={{ mask, WebkitMask: mask }} />;
}
