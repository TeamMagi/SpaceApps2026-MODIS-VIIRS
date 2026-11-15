/** MethodsLink — plain anchor to the methods page; closes any open drawer so it is gone when the user returns. */

import type { ReactNode } from "react";
import { methodsHash, type MethodsSection } from "../../lib/route";
import { useAppStore } from "../../store/useAppStore";

interface MethodsLinkProps {
  section?: MethodsSection;
  className?: string;
  children: ReactNode;
}

export default function MethodsLink({ section, className, children }: MethodsLinkProps) {
  const closeDrawer = useAppStore((state) => state.closeDrawer);
  return (
    <a href={methodsHash(section)} className={className} onClick={closeDrawer}>
      {children}
    </a>
  );
}
