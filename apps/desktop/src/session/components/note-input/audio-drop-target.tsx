import { Trans, useLingui } from "@lingui/react/macro";
import { Waveform } from "@phosphor-icons/react";
import type { HTMLAttributes, ReactNode } from "react";
import { useMemo } from "react";

import { cn } from "@anlg/utils";

import { AUDIO_EXTENSIONS } from "~/stt/useUploadFile";

// `tsconfig.json` steht auf `lib: ["ES2020", ...]`, deshalb kennt TypeScript
// `Intl.ListFormat` nicht (erst ES2021.Intl) -- die Laufzeit (jeder Tauri-
// Webview seit Jahren) kennt es sehr wohl. Reine additive Ergaenzung, aendert
// keinen bestehenden Typ.
declare global {
  namespace Intl {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-declaration-merging
    class ListFormat {
      constructor(
        locales?: string | string[],
        options?: {
          style?: "long" | "short" | "narrow";
          type?: "conjunction" | "disjunction" | "unit";
        },
      );
      format(list: Iterable<string>): string;
    }
  }
}

export function AudioDropTarget({
  children,
  className,
  isActive,
  targetProps,
}: {
  children: ReactNode;
  className?: string;
  isActive: boolean;
  targetProps: HTMLAttributes<HTMLDivElement>;
}) {
  const { i18n } = useLingui();
  const supportedAudioFormats = useMemo(
    () => formatAudioExtensionList(AUDIO_EXTENSIONS, i18n.locale),
    [i18n.locale],
  );

  return (
    <div {...targetProps} className={cn(["relative min-h-full", className])}>
      {isActive && (
        <div
          role="status"
          className={cn([
            "pointer-events-none absolute inset-0 z-30 flex items-center justify-center rounded-lg border border-dashed",
            "border-border/70 bg-background text-muted-foreground shadow-inner",
            "[background-image:radial-gradient(circle_at_center,_rgba(113,113,122,0.34)_1px,_transparent_1px)]",
            "[background-size:18px_18px]",
          ])}
        >
          <div className="border-border/70 bg-card/95 text-foreground flex items-center gap-3 rounded-md border px-4 py-3 shadow-sm">
            <Waveform className="text-muted-foreground size-5 shrink-0" />
            <div className="flex min-w-0 flex-col gap-0.5">
              <p className="text-sm font-medium">
                <Trans>Drop to upload and transcribe audio</Trans>
              </p>
              <p className="text-muted-foreground text-xs">
                <Trans>{supportedAudioFormats} audio</Trans>
              </p>
            </div>
          </div>
        </div>
      )}
      {children}
    </div>
  );
}

function formatAudioExtensionList(extensions: string[], locale: string) {
  const labels = extensions.map((extension) => extension.toUpperCase());
  try {
    return new Intl.ListFormat(locale, {
      style: "long",
      type: "disjunction",
    }).format(labels);
  } catch {
    return labels.join(", ");
  }
}
