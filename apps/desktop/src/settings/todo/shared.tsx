import type { ReactNode } from "react";

export type TodoProvider = {
  id: string;
  displayName: string;
  icon: ReactNode;
  filterLabel?: string;
  filterPlaceholder?: string;
  permission?: "reminders";
  platform?: "macos" | "all";
};

export const TODO_PROVIDERS: TodoProvider[] = [
  {
    id: "apple-reminders",
    displayName: "Apple Reminders",
    icon: (
      <img
        src="/assets/apple-reminders.png"
        alt="Apple Reminders"
        className="size-5 rounded-[4px] object-cover"
      />
    ),
    permission: "reminders",
    platform: "macos",
  },
];
