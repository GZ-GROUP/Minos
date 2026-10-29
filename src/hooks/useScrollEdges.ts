import { useCallback, useEffect, useRef, useState, type DependencyList } from "react";

/**
 * Tracks whether a scroll container has more content above/below, for the
 * `.scroll-fade` edge masks (global.css). Re-measures on resize and when `deps` change.
 */
export function useScrollEdges<T extends HTMLElement>(deps: DependencyList) {
  const ref = useRef<T>(null);
  const [edges, setEdges] = useState({ top: false, bottom: false });

  const update = useCallback(() => {
    const element = ref.current;
    if (!element) return;
    const top = element.scrollTop > 0;
    const bottom = element.scrollTop + element.clientHeight < element.scrollHeight - 1;
    setEdges((current) => (current.top === top && current.bottom === bottom ? current : { top, bottom }));
  }, []);

  useEffect(() => {
    update();
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return {
    ref,
    update,
    fadeProps: { "data-fade-top": edges.top, "data-fade-bottom": edges.bottom },
  };
}
