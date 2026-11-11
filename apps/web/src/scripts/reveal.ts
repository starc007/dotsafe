/** Marks each `[data-reveal]` (src/components/landing/Reveal.astro) as shown the first
 * time it comes 80px into view; the CSS in global.css fades it up. */
const observer = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      entry.target.setAttribute('data-shown', '');
      observer.unobserve(entry.target);
    }
  },
  { rootMargin: '-80px' },
);

for (const el of document.querySelectorAll('[data-reveal]')) observer.observe(el);
