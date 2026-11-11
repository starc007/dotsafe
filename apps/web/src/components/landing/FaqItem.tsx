import { useId, useState } from 'react';
import { Plus } from 'lucide-react';

/** One question that opens in place. The answer's row grows from 0fr to 1fr
 * while it fades and slides in; closing runs the same steps backwards. */
export function FaqItem({ q, a }: { q: string; a: string }) {
  const [open, setOpen] = useState(false);
  const id = useId();

  return (
    <li className="border-b border-ds-line">
      <h3>
        <button
          type="button"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => setOpen((o) => !o)}
          className="flex w-full cursor-pointer items-center justify-between gap-4 py-5 text-left text-[15px] font-medium outline-none focus-visible:text-ds-accent"
        >
          {q}
          <Plus
            aria-hidden
            className={`size-4 shrink-0 text-ds-muted transition-transform duration-[220ms] ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:transition-none ${open ? 'rotate-45' : ''}`}
          />
        </button>
      </h3>
      <div
        id={id}
        role="region"
        inert={!open}
        className={`grid transition-[grid-template-rows] duration-[220ms] ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:transition-none ${open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}`}
      >
        <div className="min-h-0 overflow-hidden">
          <p
            className={`max-w-xl pb-5 text-sm leading-relaxed text-pretty text-ds-muted transition-[opacity,translate] duration-[220ms] ease-[cubic-bezier(0.16,1,0.3,1)] motion-reduce:transition-none ${open ? 'translate-y-0 opacity-100' : '-translate-y-1 opacity-0'}`}
          >
            {a}
          </p>
        </div>
      </div>
    </li>
  );
}
