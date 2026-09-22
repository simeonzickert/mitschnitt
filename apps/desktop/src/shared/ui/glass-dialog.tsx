import type { ComponentProps } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import { DialogContent } from "@anlg/ui/components/ui/dialog";
import { cn } from "@anlg/utils";

/**
 * Der Glas-Dialog der App.
 *
 * `verbindlich` macht ihn zu einer Frage, die mit einer Entscheidung endet:
 * Escape und ein Klick daneben schliessen ihn dann nicht mehr. Das Bauteil
 * blendet sein eingebautes Schliessen-Zeichen ohnehin aus
 * (`[&>button:last-child]:hidden`) -- ohne diesen Riegel ist ein solcher
 * Dialog die schlechteste Mischung: unsichtbar fuer den, der raus will, und
 * ein Versehen fuer den, der es nicht wollte.
 *
 * Ausdruecklich ein Schalter und keine neue Grundeinstellung. Die beiden
 * anderen Verwender haben einen sichtbaren Ausgang (`Cancel`) und bleiben
 * deshalb unangetastet: `destructive-confirmation-dialog` und der
 * Kalender-Berechtigungsdialog.
 */
export function GlassDialogContent({
  className,
  verbindlich = false,
  onEscapeKeyDown,
  onInteractOutside,
  ...props
}: ComponentProps<typeof DialogContent> & { verbindlich?: boolean }) {
  return (
    <DialogContent
      overlayClassName="bg-black/40"
      onEscapeKeyDown={(event) => {
        if (verbindlich) {
          event.preventDefault();
        }
        onEscapeKeyDown?.(event);
      }}
      // Deckt beide Wege nach draussen ab: Zeigergeraet und Fokuswechsel.
      onInteractOutside={(event) => {
        if (verbindlich) {
          event.preventDefault();
        }
        onInteractOutside?.(event);
      }}
      className={cn([
        "w-[calc(100vw-48px)] max-w-[320px] gap-4 overflow-hidden rounded-[26px] p-5 sm:rounded-[26px]",
        "border-border/45 bg-card/60 backdrop-blur-2xl backdrop-saturate-150",
        "shadow-[inset_0_1px_0_rgba(255,255,255,0.4),0_24px_70px_rgba(0,0,0,0.32)]",
        "[&>button:last-child]:hidden",
        className,
      ])}
      {...props}
    />
  );
}

export function GlassDialogCancelButton({
  className,
  ...props
}: ComponentProps<typeof Button>) {
  return (
    <Button
      variant="ghost"
      className={cn([
        "border-border/70 bg-background/50 text-foreground h-8 rounded-full border px-4 text-xs font-medium shadow-[0_1px_2px_rgba(0,0,0,0.06)]",
        "hover:bg-background/80 hover:text-foreground",
        className,
      ])}
      {...props}
    />
  );
}
