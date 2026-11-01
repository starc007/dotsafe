"use client";

import { useCallback, useLayoutEffect, useRef, useState } from "react";

/**
 * Where the keyboard or the pointer last moved to: the row's id, stamped with
 * the query it was placed under.
 */
type RowCursor = { id: string; query: string };

/** The cursor's row, or -1 once the query has moved on or the row has left. */
function indexOfCursor(rows: readonly { id: string }[], query: string, cursor: RowCursor | null) {
  if (cursor === null || cursor.query !== query) return -1;
  return rows.findIndex((row) => row.id === cursor.id);
}

/**
 * The highlighted row of a list whose rows can change under it.
 *
 * Which row is highlighted is resolved during render, never in a passive
 * effect, so `aria-activedescendant` and Enter never name a row that has left
 * the list. The cursor holds the row's id, not its position, and is stamped
 * with the query so it's dropped when the query changes. `moveTo(null)` is for
 * deliberate resets, such as reopening the list.
 */
export function useRowCursor(rows: readonly { id: string }[], query: string) {
  const [cursor, setCursor] = useState<RowCursor | null>(null);
  const latest = useRef({ rows, query });
  useLayoutEffect(() => {
    latest.current = { rows, query };
  });

  const cursorRow = indexOfCursor(rows, query, cursor);
  if (cursor !== null && cursorRow < 0) setCursor(null);

  const moveTo = useCallback(
    (id: string | null) => setCursor(id === null ? null : { id, query: latest.current.query }),
    [],
  );

  const moveActive = useCallback((direction: 1 | -1) => {
    const { rows: live, query: liveQuery } = latest.current;
    const last = live.length - 1;
    if (last < 0) return;
    setCursor((current) => {
      const at = Math.max(indexOfCursor(live, liveQuery, current), 0);
      const next = Math.min(Math.max(at + direction, 0), last);
      return { id: live[next].id, query: liveQuery };
    });
  }, []);

  return { activeIndex: cursorRow < 0 ? 0 : cursorRow, moveTo, moveActive };
}
