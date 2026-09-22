import { useLingui } from "@lingui/react/macro";

import { SelectItem } from "@anlg/ui/components/ui/select";

export const PROVIDER_LIST_TOGGLE_VALUE = "__provider-list-toggle__";

// Diese Zeile muss ein ECHTES SelectItem sein, sonst erreichen Radix'
// Pfeiltasten (Roving Tabindex) sie nie -- ein <div tabIndex={0}> bekommt in
// einem offenen Select-Menue keinen Fokus. Jeder Handler ruft zuerst
// event.preventDefault(), damit Radix' composeEventHandlers(...) seinen
// eigenen handleSelect() NICHT mehr aufruft -- der wuerde sonst einen Wert
// setzen und das Menue schliessen. SelectItemProps (@radix-ui/react-select
// 2.2.6) kennt kein onSelect, das ist die API von DropdownMenu, nicht von
// Select.
export function ProviderListToggle({
  expanded,
  onToggle,
}: {
  expanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useLingui();
  const label = expanded ? t`Show fewer providers` : t`Show more providers`;

  return (
    <SelectItem
      value={PROVIDER_LIST_TOGGLE_VALUE}
      textValue={label}
      className="text-muted-foreground"
      onPointerUp={(event) => {
        event.preventDefault();
        onToggle();
      }}
      onClick={(event) => {
        event.preventDefault();
        onToggle();
      }}
      onKeyDown={(event) => {
        // Nur Enter/Leertaste loesen den Umschalter aus. Jede andere Taste
        // laeuft ungebremst durch zu Radix' eigenem onKeyDown (Content), das
        // damit Pfeiltasten-Navigation und Tippsuche weiter bedient.
        if (event.key !== "Enter" && event.key !== " ") {
          return;
        }
        event.preventDefault();
        onToggle();
      }}
    >
      {label}
    </SelectItem>
  );
}
