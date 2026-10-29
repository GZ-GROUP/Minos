import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { CaretDown, CaretUp, CaretUpDown, MagnifyingGlass, X } from "@phosphor-icons/react";
import { StateMessage } from "./StateMessage";
import { Tooltip } from "./Tooltip";
import "./DataList.css";

export interface Column<T> {
  key: string;
  header: string;
  align?: "start" | "end";
  /** Any CSS grid track size ("120px", "20%"). Columns without width share the remaining space. */
  width?: string;
  render: (row: T) => ReactNode;
  /** Makes the column sortable. */
  sortValue?: (row: T) => string | number;
}

type SortDirection = "asc" | "desc";

interface DataListProps<T> {
  /** Accessible name of the table. */
  label: string;
  columns: Column<T>[];
  rows: T[] | null;
  rowKey: (row: T) => string | number;
  loading?: boolean;
  error?: string | null;
  emptyMessage?: string;
  /** Enables the search box; returns the text a row is matched against. */
  searchText?: (row: T) => string;
  defaultSort?: { key: string; direction: SortDirection };
  /** Extra controls on the right of the toolbar (refresh button, live indicator…). */
  actions?: ReactNode;
  /** Below this width the list scrolls horizontally instead of squeezing columns. */
  minWidth?: number;
}

const SKELETON_ROWS = 6;

/**
 * Standard list view for tabular data: toolbar, search, sortable columns and
 * loading / error / empty states. Built on CSS grid with ARIA table roles so each
 * row can be a clay "pill" (tables can't shadow whole rows).
 */
export function DataList<T>({
  label,
  columns,
  rows,
  rowKey,
  loading = false,
  error = null,
  emptyMessage,
  searchText,
  defaultSort,
  actions,
  minWidth = 640,
}: DataListProps<T>) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState(defaultSort ?? null);
  const [edges, setEdges] = useState({ top: false, bottom: false });
  const searchRef = useRef<HTMLInputElement>(null);
  const headRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);

  // Fade the top/bottom edges only where there is more content to scroll to.
  const syncScroll = () => {
    const body = bodyRef.current;
    if (!body) return;
    if (headRef.current) headRef.current.scrollLeft = body.scrollLeft;
    const top = body.scrollTop > 0;
    const bottom = body.scrollTop + body.clientHeight < body.scrollHeight - 1;
    setEdges((current) => (current.top === top && current.bottom === bottom ? current : { top, bottom }));
  };

  // Ctrl/Cmd+F focuses the search box (only one list is mounted at a time).
  useEffect(() => {
    if (!searchText) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [searchText]);

  const visibleRows = useMemo(() => {
    if (!rows) return [];
    const needle = query.trim().toLowerCase();
    const filtered = needle && searchText ? rows.filter((row) => searchText(row).toLowerCase().includes(needle)) : rows;

    const column = columns.find((c) => c.key === sort?.key);
    if (!sort || !column?.sortValue) return filtered;
    const collator = new Intl.Collator(i18n.language, { numeric: true, sensitivity: "base" });
    const factor = sort.direction === "asc" ? 1 : -1;
    const value = column.sortValue;
    return [...filtered].sort((a, b) => {
      const left = value(a);
      const right = value(b);
      const result =
        typeof left === "number" && typeof right === "number" ? left - right : collator.compare(String(left), String(right));
      return result * factor;
    });
  }, [rows, query, searchText, sort, columns, i18n.language]);

  const showSkeleton = loading && !rows;
  const showError = !!error && !rows;

  // Re-evaluate the edge fades when the content or the panel size changes.
  useEffect(() => {
    syncScroll();
    const body = bodyRef.current;
    if (!body) return;
    const observer = new ResizeObserver(syncScroll);
    observer.observe(body);
    return () => observer.disconnect();
  }, [visibleRows, showError]);

  const toggleSort = (key: string) => {
    setSort((current) =>
      current?.key === key
        ? { key, direction: current.direction === "asc" ? "desc" : "asc" }
        : { key, direction: "asc" },
    );
  };

  const gridStyle = {
    "--dl-columns": columns.map((column) => column.width ?? "minmax(0, 1fr)").join(" "),
    "--dl-min-width": `${minWidth}px`,
  } as CSSProperties;

  let body: ReactNode;
  if (showSkeleton) {
    body = Array.from({ length: SKELETON_ROWS }, (_, index) => (
      <div key={index} className="data-list__row data-list__row--skeleton" aria-hidden="true">
        {columns.map((column) => (
          <span key={column.key} className="data-list__cell" data-align={column.align ?? "start"}>
            <span className="data-list__skeleton" />
          </span>
        ))}
      </div>
    ));
  } else if (visibleRows.length === 0) {
    body = <StateMessage kind="empty" message={query ? t("list.noResults", { query }) : emptyMessage} />;
  } else {
    body = visibleRows.map((row) => (
      <div key={rowKey(row)} role="row" className="data-list__row data-list__row--body">
        {columns.map((column) => (
          <div key={column.key} role="cell" className="data-list__cell" data-align={column.align ?? "start"}>
            {column.render(row)}
          </div>
        ))}
      </div>
    ));
  }

  return (
    <div className="data-list">
      <div className="data-list__toolbar">
        {searchText && (
          <label className="data-list__search">
            <MagnifyingGlass aria-hidden="true" />
            <input
              ref={searchRef}
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape" && query) {
                  event.stopPropagation();
                  setQuery("");
                }
              }}
              placeholder={t("list.search")}
              aria-label={t("list.searchLabel", { label })}
            />
            {query && (
              <button
                type="button"
                className="data-list__clear"
                aria-label={t("list.clearSearch")}
                onClick={() => {
                  setQuery("");
                  searchRef.current?.focus();
                }}
              >
                <X weight="bold" aria-hidden="true" />
              </button>
            )}
          </label>
        )}
        {rows && (
          <span className="data-list__count" aria-live="polite">
            {t("list.count", { count: visibleRows.length })}
          </span>
        )}
        {actions && <div className="data-list__actions">{actions}</div>}
      </div>

      <div className="data-list__panel clay">
        {showError ? (
          <StateMessage kind="error" error={error} />
        ) : (
          <div className="data-list__table" role="table" aria-label={label} aria-busy={showSkeleton} style={gridStyle}>
            <div ref={headRef} role="rowgroup" className="data-list__head">
              <div role="row" className="data-list__row data-list__row--head">
                {columns.map((column) => {
                  const active = sort?.key === column.key;
                  const ariaSort = active ? (sort.direction === "asc" ? "ascending" : "descending") : undefined;
                  const SortIcon = active ? (sort.direction === "asc" ? CaretUp : CaretDown) : CaretUpDown;
                  return (
                    <div
                      key={column.key}
                      role="columnheader"
                      className="data-list__cell"
                      data-align={column.align ?? "start"}
                      aria-sort={ariaSort}
                    >
                      {/* Long headings truncate; the tooltip shows the full text. */}
                      <Tooltip label={column.header} className="data-list__tip">
                        {column.sortValue ? (
                          <button
                            type="button"
                            className="data-list__sort"
                            data-active={active}
                            onClick={() => toggleSort(column.key)}
                          >
                            <span className="data-list__label">{column.header}</span>
                            <SortIcon weight="bold" aria-hidden="true" />
                          </button>
                        ) : (
                          <span className="data-list__heading" tabIndex={0}>
                            <span className="data-list__label">{column.header}</span>
                          </span>
                        )}
                      </Tooltip>
                    </div>
                  );
                })}
              </div>
            </div>
            <div
              ref={bodyRef}
              role="rowgroup"
              className="data-list__body scroll-fade"
              data-fade-top={edges.top}
              data-fade-bottom={edges.bottom}
              onScroll={syncScroll}
            >
              <div className="data-list__rows">{body}</div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

interface CellTextProps {
  primary: ReactNode;
  secondary?: ReactNode;
  /** Monospace for addresses and identifiers. */
  mono?: boolean;
}

/** One- or two-line cell content that truncates with an ellipsis. */
export function CellText({ primary, secondary, mono = false }: CellTextProps) {
  return (
    <span className="cell-text" data-mono={mono}>
      <span className="cell-text__primary" title={typeof primary === "string" ? primary : undefined}>
        {primary}
      </span>
      {secondary && (
        <span className="cell-text__secondary" title={typeof secondary === "string" ? secondary : undefined}>
          {secondary}
        </span>
      )}
    </span>
  );
}
