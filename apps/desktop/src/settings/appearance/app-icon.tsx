import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { getIdentifier } from "@tauri-apps/api/app";
import { platform } from "@tauri-apps/plugin-os";

import { cn } from "@anlg/utils";

import { useSetSettingValue } from "~/settings/queries";
import { useConfigValue } from "~/shared/config";
import {
  type AppIconPreference,
  hasDarkAppIconVariant,
  normalizeAppIconPreference,
  resolveAppIconName,
} from "~/shared/theme/icon";
import { applyAppIconPreference } from "~/shared/theme/provider";
import type { ThemePreference } from "~/shared/theme/resolve";

// Nur noch die Welle (ZICK-278, 02.09.2026): "default" folgt dem Bau-Kanal,
// "stable" erzwingt das Production-Icon. Im Stable-Bau faellt "stable" als
// Duplikat von "default" heraus, es bleibt EINE Option -- und dann wird der
// Abschnitt gar nicht gezeigt. Die Liste bleibt, damit eine kuenftig
// freigegebene Variante nur hier und in shared/theme/icon.ts eingetragen
// werden muss.
const APP_ICON_OPTIONS = [
  "default",
  "stable",
] as const satisfies readonly AppIconPreference[];

const PREVIEW_CLASS =
  "size-16 scale-[1.16] transition-transform duration-150 select-none group-hover:scale-[1.21]";

export function AppIconSelector() {
  const { t } = useLingui();
  const value = normalizeAppIconPreference(useConfigValue("app_icon"));
  const storedTheme = useConfigValue("theme") as ThemePreference;
  const theme: ThemePreference =
    storedTheme === "light" || storedTheme === "dark" ? storedTheme : "system";
  const setAppIcon = useSetSettingValue("app_icon");
  // Ersatzwert, solange Tauri die Kennung noch nicht geliefert hat. Er
  // entscheidet ueber die Suffix-Pruefung in resolveAppIconName (.dev /
  // .staging / sonst stable) -- hier stand die Kennung des Ur-Produkts.
  const { data: appIdentifier = "media.zickert.mitschnitt.stable" } = useQuery({
    queryKey: ["tauri", "app-identifier"],
    queryFn: getIdentifier,
    staleTime: Infinity,
  });
  const labels = {
    default: t`Default`,
    stable: t`Production`,
  };
  const defaultIconName = resolveAppIconName("default", appIdentifier);
  const selectedIconName = resolveAppIconName(value, appIdentifier);
  const options = APP_ICON_OPTIONS.filter(
    (option) => option === "default" || option !== defaultIconName,
  );

  if (platform() !== "macos") {
    return null;
  }

  // Ein Waehler mit einer Option ist keiner. Leere ist kein Defekt.
  if (options.length < 2) {
    return null;
  }

  return (
    <section className="flex flex-col gap-4">
      <div>
        <h3 className="text-lg font-semibold">
          <Trans>App icon</Trans>
        </h3>
        <p className="text-muted-foreground mt-1 text-sm">
          <Trans>Choose how Mitschnitt appears in the Dock.</Trans>
        </p>
      </div>
      <div
        role="radiogroup"
        aria-label={t`App icon`}
        className="flex flex-wrap gap-3"
      >
        {options.map((option) => {
          const selected =
            resolveAppIconName(option, appIdentifier) === selectedIconName;
          const previewName = resolveAppIconName(option, appIdentifier);
          const hasDarkVariant = hasDarkAppIconVariant(previewName);
          return (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-label={labels[option]}
              title={labels[option]}
              className={cn([
                "group text-foreground focus-visible:ring-ring focus-visible:ring-offset-background relative flex cursor-pointer items-center justify-center rounded-[22px] border bg-transparent p-0.5 transition-[border-color,scale] duration-150 focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:outline-none active:scale-[0.98] disabled:cursor-wait",
                selected
                  ? "border-transparent"
                  : "border-border hover:border-foreground/30",
              ])}
              onClick={() => {
                void applyAppIconPreference(option, theme);
                setAppIcon(option);
              }}
            >
              <span
                aria-hidden
                className={cn([
                  "pointer-events-none absolute inset-x-2.5 bottom-0 h-2 rounded-full",
                  "bg-black/35 blur-[6px] dark:bg-white/25",
                  "transition-opacity duration-150",
                  selected ? "opacity-100" : "opacity-0",
                ])}
              />
              <span
                className={cn([
                  "flex size-16 overflow-hidden rounded-[18px]",
                  "transition-transform duration-150",
                  selected && "-translate-y-1",
                ])}
              >
                {theme === "system" && hasDarkVariant ? (
                  <picture>
                    <source
                      media="(prefers-color-scheme: dark)"
                      srcSet={`/assets/app-icons/${previewName}-dark.png`}
                    />
                    <img
                      src={`/assets/app-icons/${previewName}-light.png`}
                      alt=""
                      draggable={false}
                      className={PREVIEW_CLASS}
                    />
                  </picture>
                ) : (
                  <img
                    src={`/assets/app-icons/${previewName}${
                      hasDarkVariant ? `-${theme}` : ""
                    }.png`}
                    alt=""
                    draggable={false}
                    className={PREVIEW_CLASS}
                  />
                )}
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
