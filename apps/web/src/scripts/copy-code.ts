/** The copy button on every code block (src/components/docs/CodeBlock.tsx):
 * one listener for the page. The button holds its code in `data-copy-code`,
 * and shows a check for a moment once it's copied. */
const timers = new WeakMap<HTMLElement, ReturnType<typeof setTimeout>>();

document.addEventListener('click', async (e) => {
  const button = (e.target as Element | null)?.closest?.<HTMLButtonElement>('[data-copy-code]');
  if (!button) return;
  try {
    await navigator.clipboard.writeText(button.dataset.copyCode ?? '');
  } catch {
    // Clipboard blocked: the text is still selectable.
    return;
  }
  button.dataset.copied = '';
  button.setAttribute('aria-label', 'Copied');
  button.title = 'Copied';
  clearTimeout(timers.get(button));
  timers.set(
    button,
    setTimeout(() => {
      delete button.dataset.copied;
      button.setAttribute('aria-label', 'Copy');
      button.title = 'Copy';
    }, 1500),
  );
});
