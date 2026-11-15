/**
 * Drawer — accessible modal side panel, the base for SettingsDrawer, BriefingDrawer and CellDiagnosticDrawer.
 *
 * Behaviour (implemented): open when `store.openDrawer === id`; rendered in a portal on
 * <body>; role="dialog" + aria-modal + aria-labelledby the title; focus moves into the panel on
 * open, Tab/Shift+Tab are trapped inside, Escape and the backdrop close it, focus returns to the
 * element that opened it. Slides in from the right (bottom sheet under 640 px); with reduced
 * motion it only fades. Copy: closeAction.
 */

import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useCopy } from "../../hooks/useCopy";
import { useAppStore } from "../../store/useAppStore";
import type { DrawerId } from "../../types";
import styles from "./Drawer.module.css";

export interface DrawerProps {
  id: Exclude<DrawerId, "receipt">;
  title: string;
  /** Optional one-line lead under the title, also used as aria-describedby. */
  description?: string;
  /** Sticky action row at the bottom of the panel. */
  footer?: ReactNode;
  children: ReactNode;
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

const DRAWER_SECONDS = 0.24;
const REDUCED_SECONDS = 0.15;
/** Must match the breakpoint in Drawer.module.css. */
const BOTTOM_SHEET_QUERY = "(max-width: 639px)";

export default function Drawer({ id, title, description, footer, children }: DrawerProps) {
  const open = useAppStore((state) => state.openDrawer === id);
  const closeDrawer = useAppStore((state) => state.closeDrawer);
  const { t } = useCopy();
  const reduceMotion = useReducedMotion();
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panelRef.current?.focus();
    return () => {
      if (previous?.isConnected) previous.focus();
    };
  }, [open]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeDrawer();
      return;
    }
    if (event.key !== "Tab" || !panelRef.current) return;
    const focusables = Array.from(panelRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (focusables.length === 0) {
      event.preventDefault();
      return;
    }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || active === panelRef.current)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const seconds = reduceMotion ? REDUCED_SECONDS : DRAWER_SECONDS;
  const bottomSheet = typeof window !== "undefined" && window.matchMedia(BOTTOM_SHEET_QUERY).matches;
  const offscreen = reduceMotion ? { opacity: 0 } : bottomSheet ? { opacity: 1, y: "100%" } : { opacity: 1, x: "100%" };

  return createPortal(
    <AnimatePresence>
      {open && (
        <div className={styles.root} key={id} data-drawer={id}>
          <motion.div
            className={styles.backdrop}
            aria-hidden="true"
            onClick={closeDrawer}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: seconds }}
          />
          <motion.div
            ref={panelRef}
            className={styles.panel}
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            aria-describedby={description ? descriptionId : undefined}
            tabIndex={-1}
            onKeyDown={onKeyDown}
            initial={offscreen}
            animate={{ opacity: 1, x: 0, y: 0 }}
            exit={offscreen}
            transition={{ duration: seconds, ease: [0.2, 0, 0, 1] }}
          >
            <div className={styles.head}>
              <h2 id={titleId} className={styles.title}>
                {title}
              </h2>
              <button type="button" className={styles.close} onClick={closeDrawer}>
                {t("closeAction")}
              </button>
            </div>
            {description && (
              <p id={descriptionId} className={styles.description}>
                {description}
              </p>
            )}
            <div className={styles.body}>{children}</div>
            {footer && <div className={styles.footer}>{footer}</div>}
          </motion.div>
        </div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
