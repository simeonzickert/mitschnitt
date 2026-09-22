export const FLOATING_BAR_INSET = 4;
export const FLOATING_BAR_COMPACT_HEIGHT = 38;
export const FLOATING_BAR_COMPACT_STOP_WIDTH = 62;
export const FLOATING_BAR_COMPACT_SOLO_STOP_WIDTH = 68;
export const FLOATING_BAR_COMPACT_ICON_SIZE = 30;
export const FLOATING_BAR_COMPACT_GAP = 3;
export const FLOATING_BAR_COMPACT_HORIZONTAL_PADDING = 4;
export const FLOATING_BAR_EXPANDED_WIDTH = 360;
export const FLOATING_BAR_EXPANDED_HEIGHT = 430;
export const FLOATING_BAR_HOVER_HANDLE_HEIGHT = 12;
export const FLOATING_BAR_HOVER_HANDLE_TOP_PADDING = 7;
export const FLOATING_BAR_HOVER_HANDLE_GAP = 2;
export const FLOATING_BAR_HOVER_HANDLE_RESERVED_HEIGHT =
  FLOATING_BAR_HOVER_HANDLE_TOP_PADDING +
  FLOATING_BAR_HOVER_HANDLE_HEIGHT +
  FLOATING_BAR_HOVER_HANDLE_GAP;
/**
 * Height of the dead-track warning strip inside the expanded panel.
 *
 * Internal only: it eats into the transcript list, so the panel keeps its
 * existing outer height and no window resize (and no Rust layout mirror) is
 * involved.
 */
export const FLOATING_BAR_TRACK_WARNING_HEIGHT = 22;
export const FLOATING_BAR_CONTROL_RADIUS = 10;
export const FLOATING_BAR_COMPACT_RADIUS = 14;
export const FLOATING_BAR_EXPANDED_RADIUS = 21;

export function compactControlsWidth(showsExpand: boolean) {
  return showsExpand
    ? FLOATING_BAR_COMPACT_STOP_WIDTH +
        FLOATING_BAR_COMPACT_GAP +
        FLOATING_BAR_COMPACT_ICON_SIZE
    : FLOATING_BAR_COMPACT_SOLO_STOP_WIDTH;
}

export function compactWidth(showsExpand: boolean) {
  return (
    compactControlsWidth(showsExpand) +
    FLOATING_BAR_COMPACT_HORIZONTAL_PADDING * 2
  );
}
