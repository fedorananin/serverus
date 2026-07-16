/**
 * Clipboard text reaches the PTY verbatim, and both LF and CR arrive there as
 * Enter: xterm normalizes `\r?\n` to `\r`, and a lone `\r` passes through
 * untouched. Either one can therefore run a pasted command before the user has
 * read it (pastejacking), so both must route through the confirmation dialog.
 *
 * Bracketed paste defends the common case but not a raw `sh`, a `sudo`
 * password prompt, or any program that disabled the mode — exactly where a
 * stray command costs the most.
 */
const EXECUTING_CHARACTER = /[\n\r]/u;

export function needsPasteConfirmation(text: string): boolean {
  return EXECUTING_CHARACTER.test(text);
}
