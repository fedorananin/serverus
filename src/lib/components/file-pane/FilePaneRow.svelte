<script lang="ts">
  import type { RemoteEntry, S3AclStatus, SizeFormat } from "$lib/api";
  import { formatMtime, formatPermissions, formatSize } from "$lib/format";

  interface Props {
    entry: RemoteEntry;
    selected: boolean;
    s3: boolean;
    aclStatus?: S3AclStatus;
    sizeFormat: SizeFormat;
    onclick: (event: MouseEvent) => void;
    ondoubleclick: () => void;
    oncontextmenu: (event: MouseEvent) => void;
    onpointerdown: (event: PointerEvent) => void;
  }

  let {
    entry,
    selected,
    s3,
    aclStatus,
    sizeFormat,
    onclick,
    ondoubleclick,
    oncontextmenu,
    onpointerdown,
  }: Props = $props();

  function formatAcl(status: S3AclStatus | undefined): string {
    switch (status) {
      case "public":
        return "public";
      case "private":
        return "private";
      case "unknown":
        return "?";
      default:
        return "…";
    }
  }
</script>

<div
  class="row mono"
  class:selected
  role="option"
  aria-label={entry.name}
  aria-selected={selected}
  data-access={s3 && !entry.is_dir ? formatAcl(aclStatus) : undefined}
  tabindex="-1"
  {onclick}
  ondblclick={ondoubleclick}
  {oncontextmenu}
  {onpointerdown}
  onkeydown={() => {}}
>
  <span class="cell name">
    <span class="icon">{entry.is_dir ? "📁" : "📄"}</span>
    {entry.name}{entry.is_symlink ? " →" : ""}
  </span>
  <span class="cell size">{entry.is_dir ? "—" : formatSize(entry.size, sizeFormat)}</span>
  <span class="cell date">{formatMtime(entry.mtime)}</span>
  {#if s3}
    <span class="cell perm" class:acl-public={aclStatus === "public"}>
      {entry.is_dir ? "" : formatAcl(aclStatus)}
    </span>
  {:else}
    <span class="cell perm">{formatPermissions(entry.permissions)}</span>
  {/if}
</div>

<style>
  .row {
    display: flex;
    height: 24px;
    align-items: center;
    padding: 0 8px;
    font-size: 11.5px;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }

  .row:hover {
    background: var(--bg-2);
  }

  .row.selected {
    background: var(--accent-subtle);
  }

  .cell {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell.name {
    flex: 1;
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .icon {
    font-size: 11px;
  }

  .cell.size {
    width: 66px;
    text-align: right;
    color: var(--text-1);
  }

  .cell.date {
    width: 78px;
    padding-left: 8px;
    color: var(--text-1);
  }

  .cell.perm {
    width: 80px;
    padding-left: 8px;
    color: var(--text-2);
  }

  .cell.perm.acl-public {
    color: var(--warning);
  }
</style>
