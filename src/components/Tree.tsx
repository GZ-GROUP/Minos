import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ArrowUp, CaretRight, FolderOpen, House, type Icon } from "@phosphor-icons/react";
import { Button } from "./Button";
import { StateMessage } from "./StateMessage";
import { Tooltip } from "./Tooltip";
import { useScrollEdges } from "../hooks/useScrollEdges";
import { fileIcon } from "../lib/fileIcons";
import { formatBytes, formatDate } from "../lib/format";
import "./Tree.css";

export interface TreeEntry {
  /** Unique id, usually the absolute path. Folders navigate to it. */
  id: string;
  name: string;
  kind: "folder" | "file";
  size?: number;
  modifiedMs?: number | null;
  hidden?: boolean;
}

export interface TreeCrumb {
  name: string;
  /** Location to navigate to when the crumb is clicked. */
  path: string;
}

export type TreeAction = "rename" | "delete";

interface TreeProps {
  /** Accessible name of the tree. */
  label: string;
  /** Navigation root: crumbs above it are dropped and "Up" stops here. */
  home: string;
  /** From the root to the current folder (last). */
  crumbs: TreeCrumb[] | null;
  /** Children of the current folder. */
  entries: TreeEntry[] | null;
  loading?: boolean;
  /** Already-translated error message. */
  error?: string | null;
  emptyMessage?: string;
  selected: string | null;
  onSelect: (id: string | null) => void;
  onNavigate: (path: string) => void;
  /** Keyboard shortcuts: F2 → rename, Delete → delete. */
  onAction?: (action: TreeAction, entry: TreeEntry) => void;
  iconFor?: (entry: TreeEntry) => Icon;
  /** View controls shown at the right end of the breadcrumb bar. */
  actions?: ReactNode;
}

const SKELETON_ROWS = 5;

/**
 * Shows the current folder as a tree: the folder is the root node and its contents
 * hang from it. Double-click (or Enter / →) on a folder replaces the view with it;
 * the breadcrumb and "Up" move back, never above `home`.
 */
export function Tree({
  label,
  home,
  crumbs,
  entries,
  loading = false,
  error = null,
  emptyMessage,
  selected,
  onSelect,
  onNavigate,
  onAction,
  iconFor = (entry) => fileIcon(entry.name, entry.kind),
  actions,
}: TreeProps) {
  const { t } = useTranslation();
  const [active, setActive] = useState(0);
  const itemRefs = useRef<(HTMLLIElement | null)[]>([]);
  const treeHasFocus = useRef(false);
  const crumbsRef = useRef<HTMLOListElement>(null);
  const { ref: scrollRef, update: updateEdges, fadeProps } = useScrollEdges<HTMLDivElement>([entries]);

  // Home guard: never show (or allow navigating to) anything above `home`.
  const trail = useMemo(() => {
    if (!crumbs) return [];
    const homeIndex = crumbs.findIndex((crumb) => crumb.path === home);
    return homeIndex > 0 ? crumbs.slice(homeIndex) : crumbs;
  }, [crumbs, home]);
  const current = trail[trail.length - 1];
  const parent = trail.length > 1 ? trail[trail.length - 2] : null;
  const items = entries ?? [];

  // New folder: start at the top and keep the breadcrumb scrolled to its end.
  useEffect(() => {
    setActive(0);
    crumbsRef.current?.scrollTo({ left: crumbsRef.current.scrollWidth });
  }, [current?.path]);

  // Follow selection changes made outside the tree (e.g. after creating an item).
  useEffect(() => {
    const index = items.findIndex((entry) => entry.id === selected);
    if (index < 0) return;
    setActive(index);
    itemRefs.current[index]?.scrollIntoView({ block: "nearest" });
  }, [selected, entries]);

  // Keep keyboard focus inside the tree when its content is replaced.
  useEffect(() => {
    if (treeHasFocus.current) itemRefs.current[Math.min(active, items.length - 1)]?.focus();
  }, [entries]);

  const focusItem = (index: number) => {
    const clamped = Math.max(0, Math.min(items.length - 1, index));
    setActive(clamped);
    onSelect(items[clamped]?.id ?? null);
    itemRefs.current[clamped]?.focus();
  };

  const open = (entry: TreeEntry) => {
    if (entry.kind === "folder") onNavigate(entry.id);
  };

  const goUp = () => parent && onNavigate(parent.path);

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const entry = items[active];
    const handled = (() => {
      switch (event.key) {
        case "ArrowDown":
          return focusItem(active + 1), true;
        case "ArrowUp":
          if (event.altKey) return goUp(), true;
          return focusItem(active - 1), true;
        case "Home":
          return focusItem(0), true;
        case "End":
          return focusItem(items.length - 1), true;
        case "Enter":
        case "ArrowRight":
          return entry ? (open(entry), true) : false;
        case "Backspace":
        case "ArrowLeft":
          return goUp(), true;
        case "F2":
          return entry && onAction ? (onAction("rename", entry), true) : false;
        case "Delete":
          return entry && onAction ? (onAction("delete", entry), true) : false;
        default:
          return false;
      }
    })();
    if (handled) event.preventDefault();
  };

  let children;
  if (loading && !entries) {
    children = Array.from({ length: SKELETON_ROWS }, (_, index) => (
      <li key={index} className="tree__node" aria-hidden="true">
        <div className="tree__row tree__row--skeleton">
          <span className="tree__skeleton tree__skeleton--icon" />
          <span className="tree__skeleton" />
        </div>
      </li>
    ));
  } else if (items.length === 0) {
    children = (
      <li className="tree__node tree__node--empty">
        <div className="tree__empty">{emptyMessage ?? t("tree.empty")}</div>
      </li>
    );
  } else {
    children = items.map((entry, index) => {
      const EntryIcon = iconFor(entry);
      const isSelected = entry.id === selected;
      return (
        <li
          key={entry.id}
          ref={(element) => {
            itemRefs.current[index] = element;
          }}
          role="treeitem"
          aria-level={2}
          aria-setsize={items.length}
          aria-posinset={index + 1}
          aria-selected={isSelected}
          aria-expanded={entry.kind === "folder" ? false : undefined}
          tabIndex={index === active ? 0 : -1}
          className="tree__node"
          data-kind={entry.kind}
          data-hidden={entry.hidden || undefined}
          onClick={() => {
            setActive(index);
            onSelect(entry.id);
          }}
          onDoubleClick={() => open(entry)}
          onFocus={() => setActive(index)}
        >
          <div className="tree__row" data-selected={isSelected}>
            <span className="tree__icon" aria-hidden="true">
              <EntryIcon />
            </span>
            <span className="tree__name" title={entry.name}>
              {entry.name}
            </span>
            <span className="tree__meta">
              {entry.kind === "folder" ? t("tree.folder") : formatBytes(entry.size ?? 0)}
            </span>
            <span className="tree__meta tree__meta--date">
              {entry.modifiedMs ? formatDate(entry.modifiedMs) : ""}
            </span>
          </div>
        </li>
      );
    });
  }

  return (
    <div className="tree">
      <nav className="tree__bar" aria-label={t("tree.breadcrumb")}>
        <Button
          iconOnly
          size="sm"
          variant="ghost"
          icon={ArrowUp}
          label={t("tree.up")}
          disabled={!parent}
          onClick={goUp}
        />
        <ol ref={crumbsRef} className="tree__crumbs">
          {trail.map((crumb, index) => {
            const isCurrent = index === trail.length - 1;
            const content = (
              <>
                {index === 0 && <House weight="fill" aria-hidden="true" />}
                <span className="tree__crumb-label" data-truncate>
                  {crumb.name}
                </span>
              </>
            );
            return (
              <li key={crumb.path} className="tree__crumb-item">
                {index > 0 && <CaretRight className="tree__crumb-sep" weight="bold" aria-hidden="true" />}
                <Tooltip label={crumb.name} whenTruncated className="tree__crumb-tip">
                  {isCurrent ? (
                    <span className="tree__crumb" data-current aria-current="location">
                      {content}
                    </span>
                  ) : (
                    <button type="button" className="tree__crumb" onClick={() => onNavigate(crumb.path)}>
                      {content}
                    </button>
                  )}
                </Tooltip>
              </li>
            );
          })}
        </ol>
        {actions && <div className="tree__bar-actions">{actions}</div>}
      </nav>

      <div className="tree__panel clay">
        {error ? (
          <StateMessage kind="error" message={error} />
        ) : (
          <div
            ref={scrollRef}
            className="tree__scroll scroll-fade"
            {...fadeProps}
            onScroll={updateEdges}
            onClick={(event) => {
              // Clicking empty space clears the selection.
              if (event.target === event.currentTarget) onSelect(null);
            }}
          >
            <ul
              role="tree"
              aria-label={label}
              aria-busy={loading || undefined}
              className="tree__root"
              onKeyDown={onKeyDown}
              onFocus={() => (treeHasFocus.current = true)}
              onBlur={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget)) treeHasFocus.current = false;
              }}
            >
              <li role="treeitem" aria-level={1} aria-expanded="true" className="tree__node tree__node--root">
                <div className="tree__row tree__row--root">
                  <span className="tree__icon" aria-hidden="true">
                    <FolderOpen />
                  </span>
                  <span className="tree__name">{current?.name ?? ""}</span>
                  <span className="tree__meta">{entries ? t("tree.items", { count: items.length }) : ""}</span>
                </div>
                <ul role="group" className="tree__children">
                  {children}
                </ul>
              </li>
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
