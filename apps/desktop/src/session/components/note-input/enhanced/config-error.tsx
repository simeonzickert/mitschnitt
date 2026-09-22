import { Trans } from "@lingui/react/macro";

import { Button } from "@anlg/ui/components/ui/button";

import { useTabs } from "~/store/zustand/tabs";

export function ConfigError() {
  const openNew = useTabs((state) => state.openNew);

  return (
    <div
      role="alert"
      className="flex h-full min-h-[400px] flex-col items-center justify-center px-6"
    >
      <div className="mb-6 flex max-w-md flex-col gap-2 text-center">
        <p className="text-base font-medium">
          <Trans>Set up AI summaries</Trans>
        </p>
        <p className="text-muted-foreground text-sm leading-relaxed">
          <Trans>
            Choose a model under Intelligence to generate a summary from this
            transcript.
          </Trans>
        </p>
      </div>
      {/*
        Mitschnitt-Fork. Hier stand ein zweiter Knopf "Get Pro", der auf den
        Einstellungs-Reiter "account" zeigte. Den Reiter gibt es in diesem Fork
        nicht mehr: `SettingsView` kennt keinen `case "account"` und faellt
        stumm auf die allgemeine Seite zurueck. Der Knopf war damit ein
        Versprechen auf ein Abo, das es nicht gibt, mit einem Ziel, das es
        nicht gibt -- und er erscheint auf einer frischen Installation beim
        ersten Meeting, weil dort noch kein Modell eingerichtet ist. Der Weg
        ueber "Intelligence" ist der einzige echte, deshalb ist er jetzt der
        einzige Knopf.
      */}
      <Button
        className="shadow-none"
        onClick={() =>
          openNew({ type: "settings", state: { tab: "intelligence" } })
        }
      >
        <Trans>Open Intelligence settings</Trans>
      </Button>
    </div>
  );
}
