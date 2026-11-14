/**
 * RegionPicker — searchable country/region combobox popover in the Header.
 *
 * Replaces the horizontal chip row with one trigger button opening a WAI-ARIA APG
 * combobox/listbox popover (anchored under the trigger on >= 640 px, full-width below the
 * header on < 640 px). Supports Turkish diacritic- and case-insensitive filtering,
 * Intl.Collator(lang) ordering, keyboard navigation, and disabled tooLarge entries.
 */

import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { formatRegionName } from "../../content/copy";
import { useCopy } from "../../hooks/useCopy";
import { useRegions } from "../../hooks/useRegionData";
import { useAppStore } from "../../store/useAppStore";
import type { RegionId, RegionSummary } from "../../types";
import styles from "./RegionPicker.module.css";

interface SortedRegion {
  readonly summary: RegionSummary;
  readonly displayName: string;
  readonly searchKey: string;
}

function normalizeSearchText(text: string): string {
  // Map Turkish dotted/dotless I explicitly before NFD so "Irak"/"irak" and "İran"/"iran" fold identically.
  return text
    .replace(/\u0130/g, "i")
    .replace(/\u0131/g, "i")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase();
}

function formatYearSpan(span: readonly [number, number]): string {
  return span[0] === span[1] ? String(span[0]) : `${span[0]}–${span[1]}`;
}

function optionDomId(listboxId: string, regionId: RegionId): string {
  return `${listboxId}-opt-${regionId.replace(/[^a-zA-Z0-9_-]/g, "_")}`;
}

function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      className={styles.chevron}
      data-open={open ? "true" : undefined}
      viewBox="0 0 12 12"
      width="12"
      height="12"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M2.5 4.25L6 7.75L9.5 4.25"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export default function RegionPicker() {
  const { t, fmt, lang } = useCopy();
  const { status, regionsEmpty, regions, retry } = useRegions();
  const region = useAppStore((state) => state.region);
  const setRegion = useAppStore((state) => state.setRegion);

  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeId, setActiveId] = useState<RegionId | null>(null);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const optionRefs = useRef<Map<RegionId, HTMLLIElement>>(new Map());
  const activeIdRef = useRef<RegionId | null>(null);

  const popoverId = useId();
  const listboxId = useId();

  const sortedRegions = useMemo<readonly SortedRegion[]>(() => {
    const collator = new Intl.Collator(lang);
    return regions
      .map((summary) => {
        const displayName = formatRegionName(summary, lang);
        return {
          summary,
          displayName,
          searchKey: normalizeSearchText(displayName),
        };
      })
      .sort((a, b) => collator.compare(a.displayName, b.displayName));
  }, [regions, lang]);

  const filteredRegions = useMemo<readonly SortedRegion[]>(() => {
    const normalizedQuery = normalizeSearchText(query.trim());
    if (!normalizedQuery) return sortedRegions;
    return sortedRegions.filter((item) => item.searchKey.includes(normalizedQuery));
  }, [sortedRegions, query]);

  const enabledFiltered = useMemo<readonly SortedRegion[]>(
    () => filteredRegions.filter((item) => !item.summary.tooLarge),
    [filteredRegions],
  );

  const resolvedActiveId: RegionId | null = useMemo(() => {
    if (activeId && enabledFiltered.some((item) => item.summary.id === activeId)) {
      return activeId;
    }
    return enabledFiltered[0]?.summary.id ?? null;
  }, [activeId, enabledFiltered]);

  activeIdRef.current = resolvedActiveId;

  const closePopover = useCallback((restoreFocus: boolean) => {
    setIsOpen(false);
    setQuery("");
    setActiveId(null);
    if (restoreFocus) {
      triggerRef.current?.focus();
    }
  }, []);

  const openPopover = useCallback(
    (options?: { initialQuery?: string; initialDirection?: "first" | "last" }) => {
      const nextQuery = options?.initialQuery ?? "";
      const normalized = normalizeSearchText(nextQuery.trim());
      const matchingEnabled = sortedRegions.filter(
        (item) => !item.summary.tooLarge && (!normalized || item.searchKey.includes(normalized)),
      );
      let nextActive: RegionId | null = null;
      if (options?.initialDirection === "last") {
        nextActive = matchingEnabled[matchingEnabled.length - 1]?.summary.id ?? null;
      } else if (options?.initialDirection === "first" || normalized) {
        nextActive = matchingEnabled[0]?.summary.id ?? null;
      } else {
        const currentEnabled = matchingEnabled.find((item) => item.summary.id === region);
        nextActive = currentEnabled?.summary.id ?? matchingEnabled[0]?.summary.id ?? null;
      }
      setQuery(nextQuery);
      setActiveId(nextActive);
      setIsOpen(true);
    },
    [sortedRegions, region],
  );

  const selectOption = useCallback(
    (selectedId: RegionId) => {
      setRegion(selectedId);
      closePopover(true);
    },
    [setRegion, closePopover],
  );

  useEffect(() => {
    if (!isOpen) return;
    const input = inputRef.current;
    if (input) {
      input.focus();
      const len = input.value.length;
      input.setSelectionRange(len, len);
    }
    if (activeIdRef.current) {
      optionRefs.current.get(activeIdRef.current)?.scrollIntoView({ block: "nearest" });
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        closePopover(false);
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [isOpen, closePopover]);

  const moveActive = (delta: 1 | -1) => {
    if (enabledFiltered.length === 0) return;
    const currentIndex = resolvedActiveId
      ? enabledFiltered.findIndex((item) => item.summary.id === resolvedActiveId)
      : -1;
    let nextIndex: number;
    if (currentIndex === -1) {
      nextIndex = delta === 1 ? 0 : enabledFiltered.length - 1;
    } else {
      nextIndex = (currentIndex + delta + enabledFiltered.length) % enabledFiltered.length;
    }
    const nextId = enabledFiltered[nextIndex].summary.id;
    setActiveId(nextId);
    optionRefs.current.get(nextId)?.scrollIntoView({ block: "nearest" });
  };

  const moveToEdge = (edge: "first" | "last") => {
    if (enabledFiltered.length === 0) return;
    const nextId =
      edge === "first" ? enabledFiltered[0].summary.id : enabledFiltered[enabledFiltered.length - 1].summary.id;
    setActiveId(nextId);
    optionRefs.current.get(nextId)?.scrollIntoView({ block: "nearest" });
  };

  const handleTriggerKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (isOpen) {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closePopover(true);
      }
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      openPopover({ initialDirection: "first" });
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      openPopover({ initialDirection: "last" });
      return;
    }
    if (event.key.length === 1 && event.key !== " " && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      openPopover({ initialQuery: event.key });
    }
  };

  const handleInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveActive(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveActive(-1);
        break;
      case "Home":
        event.preventDefault();
        moveToEdge("first");
        break;
      case "End":
        event.preventDefault();
        moveToEdge("last");
        break;
      case "Enter":
        event.preventDefault();
        if (resolvedActiveId) {
          selectOption(resolvedActiveId);
        }
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        closePopover(true);
        break;
      case "Tab":
        if (event.shiftKey) {
          event.preventDefault();
          closePopover(true);
        } else {
          // Return focus to trigger synchronously so native Tab moves to the next header control.
          triggerRef.current?.focus();
          closePopover(false);
        }
        break;
    }
  };

  const handleContainerKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (isOpen && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closePopover(true);
    }
  };

  if (regions.length === 0) {
    if (status === "error") {
      return (
        <div className={styles.status} role="status">
          <span className={styles.statusText}>{t("errorBadge")}</span>
          <button type="button" className={styles.retryButton} onClick={retry}>
            {t("retryAction")}
          </button>
        </div>
      );
    }
    if (status === "loading") {
      return (
        <div className={styles.picker}>
          <button
            type="button"
            className={styles.trigger}
            disabled
            aria-expanded={false}
            aria-label={t("regionTriggerAria", { label: t("regionPickerLabel"), value: t("loadingRegions") })}
          >
            <span className={styles.triggerLabel}>{t("loadingRegions")}</span>
            <Chevron open={false} />
          </button>
        </div>
      );
    }
    return (
      <div className={styles.status} role="status">
        <span className={styles.statusText}>{t("regionNoneAvailable")}</span>
        {regionsEmpty && (
          <button type="button" className={styles.retryButton} onClick={retry}>
            {t("retryAction")}
          </button>
        )}
      </div>
    );
  }

  const currentSummary = regions.find((item) => item.id === region);
  const currentDisplayName = formatRegionName(currentSummary ?? { id: region, name: region }, lang);

  return (
    <div ref={containerRef} className={styles.picker} onKeyDown={handleContainerKeyDown}>
      <button
        ref={triggerRef}
        type="button"
        className={styles.trigger}
        aria-expanded={isOpen}
        aria-controls={isOpen ? popoverId : undefined}
        aria-label={t("regionTriggerAria", { label: t("regionPickerLabel"), value: currentDisplayName })}
        onClick={() => {
          if (isOpen) closePopover(true);
          else openPopover();
        }}
        onKeyDown={handleTriggerKeyDown}
      >
        <span className={styles.triggerLabel}>{currentDisplayName}</span>
        <Chevron open={isOpen} />
      </button>

      {isOpen && (
        <div id={popoverId} className={styles.popover}>
          <div className={styles.searchRow}>
            <input
              ref={inputRef}
              type="text"
              role="combobox"
              aria-autocomplete="list"
              aria-expanded={isOpen}
              aria-controls={listboxId}
              aria-activedescendant={resolvedActiveId ? optionDomId(listboxId, resolvedActiveId) : undefined}
              aria-label={t("regionSearchLabel")}
              placeholder={t("regionSearchPlaceholder")}
              className={styles.searchInput}
              value={query}
              onChange={(event) => {
                const nextValue = event.target.value;
                setQuery(nextValue);
                const normalized = normalizeSearchText(nextValue.trim());
                const firstMatch = sortedRegions.find(
                  (item) => !item.summary.tooLarge && (!normalized || item.searchKey.includes(normalized)),
                );
                setActiveId(firstMatch?.summary.id ?? null);
              }}
              onKeyDown={handleInputKeyDown}
            />
          </div>

          <ul id={listboxId} role="listbox" aria-label={t("regionListboxLabel")} className={styles.listbox}>
            {filteredRegions.length === 0 ? (
              <li role="presentation" className={styles.emptyMatches} aria-live="polite">
                {t("regionNoMatches")}
              </li>
            ) : (
              filteredRegions.map(({ summary, displayName }) => {
                const isSelected = summary.id === region;
                const isActive = summary.id === resolvedActiveId;
                const tooLargeNote = summary.tooLarge
                  ? summary.sizeMb !== null
                    ? t("regionTooLarge", {
                        size: fmt.number(summary.sizeMb, Number.isInteger(summary.sizeMb) ? 0 : 1),
                      })
                    : t("regionTooLargeNoSize")
                  : undefined;

                const coverageParts: string[] = [];
                if (summary.modisYears) {
                  coverageParts.push(t("regionCoverageModis", { span: formatYearSpan(summary.modisYears) }));
                }
                if (summary.viirsYears) {
                  coverageParts.push(t("regionCoverageViirs", { span: formatYearSpan(summary.viirsYears) }));
                }
                const coverageText = coverageParts.join(" · ");

                const optId = optionDomId(listboxId, summary.id);
                const descId = tooLargeNote ? `${optId}-desc` : undefined;

                return (
                  <li
                    key={summary.id}
                    ref={(el) => {
                      if (el) optionRefs.current.set(summary.id, el);
                      else optionRefs.current.delete(summary.id);
                    }}
                    id={optId}
                    role="option"
                    aria-selected={isSelected}
                    aria-disabled={summary.tooLarge ? true : undefined}
                    aria-describedby={descId}
                    data-active={isActive ? "true" : undefined}
                    data-selected={isSelected ? "true" : undefined}
                    title={tooLargeNote}
                    className={styles.option}
                    onMouseDown={(event) => {
                      event.preventDefault();
                    }}
                    onMouseEnter={() => {
                      if (!summary.tooLarge) {
                        setActiveId(summary.id);
                      }
                    }}
                    onClick={() => {
                      if (!summary.tooLarge) {
                        selectOption(summary.id);
                      }
                    }}
                  >
                    <span className={styles.optionMain}>
                      <span className={styles.optionName}>{displayName}</span>
                      {(coverageText || tooLargeNote) && (
                        <span className={styles.optionMeta}>
                          {coverageText}
                          {coverageText && tooLargeNote ? " · " : ""}
                          {tooLargeNote}
                        </span>
                      )}
                    </span>
                    <span className={styles.optionCheck} aria-hidden="true">
                      {isSelected ? "✓" : ""}
                    </span>
                    {isSelected && <span className="sr-only">, {t("regionSelectedMarker")}</span>}
                    {tooLargeNote && descId && (
                      <span id={descId} className="sr-only">
                        {tooLargeNote}
                      </span>
                    )}
                  </li>
                );
              })
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
