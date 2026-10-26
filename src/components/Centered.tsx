import type { ReactNode } from "react";
import { motion } from "motion/react";
import { base } from "@/lib/motion";
import { DragRegion } from "@/components/DragRegion";
import { Surface, SurfaceBody, SurfaceHeader, surfaceDescriptionClass, surfaceTitleClass } from "@/components/Surface";

/** Layout for the pre-vault screens: one shell card centered in a draggable window. */
export function Centered({
  icon,
  title,
  subtitle,
  children,
}: {
  icon: ReactNode;
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="relative flex h-full items-center justify-center overflow-y-auto bg-background p-6">
      <DragRegion className="fixed inset-x-0 top-0 h-12" />
      <motion.main
        className="w-full max-w-[400px]"
        initial={{ opacity: 0, y: 6 }}
        animate={{ opacity: 1, y: 0 }}
        transition={base}
      >
        <Surface>
          <SurfaceHeader icon={icon}>
            <h1 className={surfaceTitleClass}>{title}</h1>
            {subtitle && <p className={surfaceDescriptionClass}>{subtitle}</p>}
          </SurfaceHeader>
          <SurfaceBody>{children}</SurfaceBody>
        </Surface>
      </motion.main>
    </div>
  );
}
