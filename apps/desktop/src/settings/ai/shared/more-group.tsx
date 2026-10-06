import { useLingui } from "@lingui/react/macro";
import { CaretDown, CaretRight } from "@phosphor-icons/react";
import { useState } from "react";

import {
  type CollapsedMore,
  splitMoreCollapsed,
  visibleMoreProviders,
} from "./provider-groups";

type Groupable = { id: string; displayName: string };

// Shared state for the collapsible "More" group (settings lists and select
// dropdowns). Collapsed by default; selected and configured providers stay
// visible, a search query shows all hits.
export function useCollapsedMore<T extends Groupable>(
  more: readonly T[],
  options: { selectedId?: string | null; configuredIds?: readonly string[] },
  searching = false,
) {
  const [expanded, setExpanded] = useState(false);
  const collapsed: CollapsedMore<T> = splitMoreCollapsed(more, options);
  const visible = visibleMoreProviders(collapsed, { expanded, searching });
  const showRest = expanded || searching;
  return {
    visible,
    pinned: collapsed.pinned,
    hiddenCount: collapsed.rest.length,
    expanded: showRest,
    // While searching everything is open, so there is nothing to toggle.
    canToggle: collapsed.rest.length > 0 && !searching,
    toggle: () => setExpanded((value) => !value),
  };
}

const LABEL_CLASS =
  "text-muted-foreground px-2 pt-1 pb-1 text-[11px] font-medium tracking-wide uppercase";

// Heading row of the "More" group in the settings lists: a plain label when
// there is nothing to fold, otherwise a button "More (N)" with a caret.
export function MoreGroupHeader({
  hiddenCount,
  expanded,
  canToggle,
  onToggle,
}: {
  hiddenCount: number;
  expanded: boolean;
  canToggle: boolean;
  onToggle: () => void;
}) {
  const { t } = useLingui();
  if (!canToggle) {
    return <div className={LABEL_CLASS}>{t`More`}</div>;
  }
  const Caret = expanded ? CaretDown : CaretRight;
  return (
    <button
      type="button"
      aria-expanded={expanded}
      onClick={onToggle}
      data-slot="more-providers-toggle"
      className={`${LABEL_CLASS} hover:text-foreground flex w-full items-center gap-1 text-left transition-colors`}
    >
      <Caret className="size-3" aria-hidden />
      {expanded ? t`More` : t`More (${hiddenCount})`}
    </button>
  );
}
