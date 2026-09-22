/**
 * How an attachment travels through markdown: `attachment://<id>`.
 *
 * Mitschnitt-Fork (G6a, Fix-Runde 2 -- Opus). An uploaded picture is stored
 * with its `attachmentId` and no `src` (the editor strips the local `asset:`
 * URL before the save, see note/portable-attachments.ts), so a note that
 * went json -> markdown -> json (session proposals, chat corrections, the
 * welcome note) came back with `![alt]()` and `attachmentId: null` -- the
 * picture was gone from the note, and since the attachment reconciler
 * exists, its file followed after the grace window. File attachments
 * already had this link form; images get it too, and both accept the ids
 * the fork actually uses: file names, not only UUIDs.
 *
 * An id is a file name inside the session's attachments folder and nothing
 * else -- no path separators, no `.`/`..`, no control characters. It is
 * percent-encoded on the way out because a space ends a markdown link
 * destination, and decoded and checked on the way in; a link that fails the
 * check is left as an ordinary link with its `src`, never as an attachment.
 */
export const PORTABLE_ATTACHMENT_PREFIX = "attachment://";

const MAX_ATTACHMENT_ID_LENGTH = 255;
// eslint-disable-next-line no-control-regex
const UNSAFE_IN_A_FILE_NAME = /[/\\\u0000-\u001f]/;

export function isPortableAttachmentId(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0) return false;
  if (value.length > MAX_ATTACHMENT_ID_LENGTH) return false;
  if (value === "." || value === "..") return false;
  return !UNSAFE_IN_A_FILE_NAME.test(value);
}

export function serializePortableAttachmentUrl(attachmentId: string): string {
  return `${PORTABLE_ATTACHMENT_PREFIX}${encodeURIComponent(attachmentId)}`;
}

export function parsePortableAttachmentUrl(
  url: string | null | undefined,
): string | null {
  if (!url || !url.startsWith(PORTABLE_ATTACHMENT_PREFIX)) return null;
  let attachmentId: string;
  try {
    attachmentId = decodeURIComponent(
      url.slice(PORTABLE_ATTACHMENT_PREFIX.length),
    );
  } catch {
    return null;
  }
  return isPortableAttachmentId(attachmentId) ? attachmentId : null;
}
