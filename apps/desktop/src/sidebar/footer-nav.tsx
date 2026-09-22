import { useLingui } from "@lingui/react/macro";
import { CalendarDots, Gear, type Icon, Users } from "@phosphor-icons/react";
import { useCallback } from "react";

import { cn } from "@anlg/utils";

import { type Tab, type TabInput, useTabs } from "~/store/zustand/tabs";

type FooterNavItem = {
  id: string;
  label: string;
  icon: Icon;
  destination: TabInput;
  isActive: (tab: Tab | null) => boolean;
};

/**
 * Fussleiste am unteren Rand der linken Seitenleiste.
 *
 * Bewusst NUR die Ziele, die es in dieser App wirklich gibt. Der von F15
 * ebenfalls gewuenschte Ordner-Knopf fehlt mit Grund: der Tab-Typ `folders`
 * lebt zwar noch in der Rust-Bindung (`plugins/windows/js/bindings.gen.ts`),
 * die zugehoerige Ansicht hat upstream aber `fa05adaf7 chore: remove folders
 * product surface (#5233)` entfernt, und `store/zustand/tabs/schema.ts`
 * schliesst den Typ seitdem ausdruecklich aus. Ein vierter Knopf waere ein
 * Knopf ins Leere - genau der Falsifikator des Claims.
 */
export function SidebarFooterNav() {
  const { t } = useLingui();
  const currentTab = useTabs((state) => state.currentTab);
  const openNew = useTabs((state) => state.openNew);

  const items: FooterNavItem[] = [
    {
      id: "calendar",
      label: t`Calendar`,
      icon: CalendarDots,
      destination: { type: "calendar" },
      isActive: (tab) => tab?.type === "calendar",
    },
    {
      id: "contacts",
      label: t`Contacts`,
      icon: Users,
      destination: { type: "contacts" },
      isActive: (tab) => tab?.type === "contacts",
    },
    {
      id: "settings",
      label: t`Settings`,
      icon: Gear,
      destination: { type: "settings" },
      isActive: (tab) => tab?.type === "settings",
    },
  ];

  const handleOpen = useCallback(
    (destination: TabInput) => {
      openNew(destination);
    },
    [openNew],
  );

  return (
    <nav
      aria-label={t`Sidebar navigation`}
      data-testid="sidebar-footer-nav"
      className={cn([
        "border-border/60 flex min-w-0 shrink-0 flex-wrap items-center gap-1",
        "border-t px-2 pt-1.5 pb-2",
      ])}
    >
      {items.map((item) => {
        const active = item.isActive(currentTab);
        const ItemIcon = item.icon;

        return (
          <button
            key={item.id}
            type="button"
            aria-label={item.label}
            title={item.label}
            aria-current={active ? "page" : undefined}
            data-active={active ? "true" : "false"}
            data-tauri-drag-region="false"
            onClick={() => handleOpen(item.destination)}
            className={cn([
              "relative flex size-7 shrink-0 items-center justify-center rounded-full",
              "transition-colors",
              "focus-visible:ring-ring focus-visible:ring-2 focus-visible:outline-hidden",
              active
                ? "bg-sidebar-accent text-foreground"
                : "text-muted-foreground hover:bg-accent hover:text-foreground",
            ])}
          >
            <ItemIcon size={16} weight={active ? "fill" : "regular"} />
          </button>
        );
      })}
    </nav>
  );
}
