import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, MouseEvent, PointerEvent as ReactPointerEvent } from "react";

const HOLD_MS = 220;
const MOVE_THRESHOLD_PX = 6;
/** Matches --duration-base, the transition that carries items to their new cells. */
const SETTLE_MS = 260;

interface DragState {
  /** Position of each sibling when the drag began. */
  cells: DOMRect[];
  from: number;
  to: number;
  /** Offset of the lifted item from its own cell. */
  dx: number;
  dy: number;
  /** Released: the lifted item is travelling to its cell. */
  settling: boolean;
}

/** The cell a sibling moves to while `from` hovers over `to`. */
function targetCell(index: number, from: number, to: number): number {
  if (from < to && index > from && index <= to) return index - 1;
  if (to < from && index >= to && index < from) return index + 1;
  return index;
}

function nearestCell(cells: DOMRect[], x: number, y: number): number {
  let nearest = 0;
  let shortest = Infinity;
  cells.forEach((cell, index) => {
    const distance = Math.hypot(cell.left + cell.width / 2 - x, cell.top + cell.height / 2 - y);
    if (distance < shortest) {
      shortest = distance;
      nearest = index;
    }
  });
  return nearest;
}

/**
 * Reorders the siblings of a grid by pressing and dragging one. Press and hold, or
 * start moving, to lift an item; a quick click still reaches the item as usual.
 * Items only move visually until they're released, when `onReorder` is called.
 */
export function useGridReorder(onReorder: (from: number, to: number) => void) {
  const [drag, setDrag] = useState<DragState | null>(null);
  const session = useRef<AbortController | null>(null);
  const timers = useRef<number[]>([]);
  const suppressClick = useRef(false);
  const latestOnReorder = useRef(onReorder);
  latestOnReorder.current = onReorder;

  useEffect(
    () => () => {
      session.current?.abort();
      session.current = null;
      timers.current.forEach(window.clearTimeout);
    },
    [],
  );

  const onPointerDown = useCallback((event: ReactPointerEvent<HTMLElement>, index: number) => {
    if (event.button !== 0 || session.current) return;
    suppressClick.current = false;

    const item = event.currentTarget;
    const { pointerId, clientX: startX, clientY: startY } = event;
    const abort = new AbortController();
    session.current = abort;

    let cells: DOMRect[] | null = null;
    let current: DragState | null = null;

    const update = (next: DragState) => {
      current = next;
      setDrag(next);
    };

    const lift = () => {
      window.clearTimeout(holdTimer);
      cells = Array.from(item.parentElement?.children ?? [], (sibling) => sibling.getBoundingClientRect());
      item.setPointerCapture(pointerId);
      suppressClick.current = true;
      update({ cells, from: index, to: index, dx: 0, dy: 0, settling: false });
    };

    const holdTimer = window.setTimeout(lift, HOLD_MS);
    timers.current.push(holdTimer);

    const stopListening = () => {
      window.clearTimeout(holdTimer);
      abort.abort();
    };

    const release = (to: number) => {
      stopListening();
      if (!current) {
        session.current = null;
        return;
      }
      const { cells: rects, from } = current;
      update({
        ...current,
        to,
        dx: rects[to].left - rects[from].left,
        dy: rects[to].top - rects[from].top,
        settling: true,
      });
      timers.current.push(
        window.setTimeout(() => {
          session.current = null;
          suppressClick.current = false;
          if (from !== to) latestOnReorder.current(from, to);
          setDrag(null);
        }, SETTLE_MS),
      );
    };

    const options = { signal: abort.signal };

    window.addEventListener(
      "pointermove",
      (move) => {
        if (move.pointerId !== pointerId) return;
        const rawX = move.clientX - startX;
        const rawY = move.clientY - startY;
        if (!cells) {
          if (Math.hypot(rawX, rawY) < MOVE_THRESHOLD_PX) return;
          lift();
        }
        if (!cells) return;

        // Stays inside the grid, so the viewport around it never clips the item.
        const own = cells[index];
        const dx = Math.min(
          Math.max(rawX, Math.min(...cells.map((cell) => cell.left)) - own.left),
          Math.max(...cells.map((cell) => cell.right)) - own.right,
        );
        const dy = Math.min(
          Math.max(rawY, Math.min(...cells.map((cell) => cell.top)) - own.top),
          Math.max(...cells.map((cell) => cell.bottom)) - own.bottom,
        );
        const to = nearestCell(cells, own.left + own.width / 2 + dx, own.top + own.height / 2 + dy);
        update({ cells, from: index, to, dx, dy, settling: false });
      },
      options,
    );

    window.addEventListener(
      "pointerup",
      (up) => {
        if (up.pointerId === pointerId) release(current?.to ?? index);
      },
      options,
    );
    window.addEventListener("pointercancel", () => release(index), options);
    window.addEventListener("blur", () => release(index), options);
    window.addEventListener(
      "keydown",
      (key) => {
        if (key.key === "Escape") release(index);
      },
      options,
    );
  }, []);

  /** A press that lifted the item isn't a click on it. */
  const onClickCapture = useCallback((event: MouseEvent) => {
    if (!suppressClick.current) return;
    suppressClick.current = false;
    event.preventDefault();
    event.stopPropagation();
  }, []);

  const itemProps = (index: number) => {
    let style: CSSProperties | undefined;
    if (drag) {
      const { cells, from, to, dx, dy } = drag;
      if (index === from) {
        style = { transform: `translate(${dx}px, ${dy}px)` };
      } else {
        const target = cells[targetCell(index, from, to)];
        const own = cells[index];
        style = { transform: `translate(${target.left - own.left}px, ${target.top - own.top}px)` };
      }
    }
    const lifted = drag?.from === index;
    return {
      style,
      "data-reordering": drag ? true : undefined,
      "data-lifted": lifted ? true : undefined,
      "data-following": lifted && !drag?.settling ? true : undefined,
      onPointerDown: (event: ReactPointerEvent<HTMLElement>) => onPointerDown(event, index),
      onClickCapture,
    };
  };

  return { dragging: drag !== null, itemProps };
}
