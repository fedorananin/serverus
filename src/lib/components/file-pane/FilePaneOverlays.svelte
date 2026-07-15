<script lang="ts">
  import type { RemoteEntry } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import type { PaneController } from "$lib/stores/pane.svelte";
  import ChmodDialog from "../ChmodDialog.svelte";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import ContextMenu from "../ContextMenu.svelte";
  import InputDialog from "../InputDialog.svelte";
  import type { FilePaneDialog, FilePaneMenu } from "./types";

  interface Props {
    pane: PaneController;
    menu: FilePaneMenu | null;
    dialog: FilePaneDialog | null;
    onclosemenu: () => void;
    onclosedialog: () => void;
    onerror: (message: string) => void;
  }

  let { pane, menu, dialog, onclosemenu, onclosedialog, onerror }: Props = $props();

  function run(operation: Promise<void>) {
    operation.catch((error) => onerror(errorMessage(error)));
  }

  async function applyChmod(
    entry: RemoteEntry,
    mode: number,
    recursive: "files" | "dirs" | "both" | null,
  ) {
    if (!recursive) {
      await pane.chmod(entry, mode);
      return;
    }
    const stack = [entry.path];
    const applyDirs = recursive !== "files";
    const applyFiles = recursive !== "dirs";
    if (applyDirs) await pane.chmod(entry, mode);
    while (stack.length) {
      const directory = stack.pop()!;
      const children =
        pane.side === "local"
          ? await unwrap(commands.localList(directory))
          : await unwrap(commands.remoteList(pane.sessionId!, directory));
      for (const child of children) {
        if (child.is_dir && !child.is_symlink) {
          stack.push(child.path);
          if (applyDirs) {
            if (pane.side === "local") await unwrap(commands.localChmod(child.path, mode));
            else await unwrap(commands.remoteChmod(pane.sessionId!, child.path, mode));
          }
        } else if (applyFiles) {
          if (pane.side === "local") await unwrap(commands.localChmod(child.path, mode));
          else await unwrap(commands.remoteChmod(pane.sessionId!, child.path, mode));
        }
      }
    }
    await pane.refresh();
  }
</script>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={onclosemenu} />
{/if}

{#if dialog?.kind === "mkdir"}
  <InputDialog
    title="New folder"
    placeholder="folder name"
    confirmLabel="Create"
    onsubmit={(name) => run(pane.mkdir(name))}
    onclose={onclosedialog}
  />
{:else if dialog?.kind === "newfile"}
  <InputDialog
    title="New file"
    placeholder="file name"
    confirmLabel="Create"
    onsubmit={(name) => run(pane.createFile(name))}
    onclose={onclosedialog}
  />
{:else if dialog?.kind === "rename"}
  <InputDialog
    title="Rename"
    initial={dialog.entry.name}
    confirmLabel="Rename"
    onsubmit={(name) => dialog?.kind === "rename" && run(pane.rename(dialog.entry, name))}
    onclose={onclosedialog}
  />
{:else if dialog?.kind === "delete"}
  <ConfirmDialog
    title="Delete"
    message={dialog.entries.length === 1
      ? `Delete "${dialog.entries[0].name}"${dialog.entries[0].is_dir ? " and all its contents" : ""}?`
      : `Delete ${dialog.entries.length} items (folders recursively)?`}
    onconfirm={() => dialog?.kind === "delete" && run(pane.deleteEntries(dialog.entries))}
    onclose={onclosedialog}
  />
{:else if dialog?.kind === "chmod"}
  <ChmodDialog
    entry={dialog.entry}
    onapply={(mode, recursive) =>
      dialog?.kind === "chmod" && run(applyChmod(dialog.entry, mode, recursive))}
    onclose={onclosedialog}
  />
{/if}
