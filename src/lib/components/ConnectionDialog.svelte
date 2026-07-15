<script lang="ts">
  import type {
    AuthMethod,
    Badge,
    FtpTlsMode,
    Protocol,
    PublicConnection,
    S3UploadAcl,
    TunnelConfig,
  } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import Modal from "./Modal.svelte";
  import AuthenticationSection from "./connection-dialog/AuthenticationSection.svelte";
  import DirectoryFields from "./connection-dialog/DirectoryFields.svelte";
  import EndpointFields from "./connection-dialog/EndpointFields.svelte";
  import MetadataFields from "./connection-dialog/MetadataFields.svelte";
  import ProtocolOptionsSection from "./connection-dialog/ProtocolOptionsSection.svelte";
  import TunnelsSection from "./connection-dialog/TunnelsSection.svelte";
  import { buildConnectionInput } from "./connection-dialog/build-connection-input";
  import "./connection-dialog/connection-dialog.css";

  interface Props {
    /** Existing connection to edit, or null to create. */
    existing: PublicConnection | null;
    /** Folder to place a new connection into. */
    parentFolder: string | null;
    onclose: () => void;
  }

  let { existing, parentFolder, onclose }: Props = $props();

  let name = $state(existing?.name ?? "");
  let badge = $state<Badge | null>(existing?.badge ?? null);
  let protocol = $state<Protocol>(existing?.protocol ?? "ssh");
  let host = $state(existing?.host ?? "");
  let port = $state(existing?.port ?? 22);
  let portTouched = $state(existing !== null);
  let authMethod = $state<AuthMethod>(existing?.auth.method ?? "password");
  let username = $state(existing?.auth.username ?? "");
  let password = $state("");
  let keyPath = $state(existing?.auth.key_path ?? "");
  /** Where the private key comes from: a file on disk, or pasted text
   *  stored inside the vault (survives machine moves/backups). */
  let keySource = $state<"file" | "text">(existing?.auth.has_key_inline ? "text" : "file");
  let keyInline = $state("");
  let keyPassphrase = $state("");
  let jumpHost = $state<string | null>(existing?.jump_host ?? null);
  let ftpTls = $state<FtpTlsMode>(existing?.ftp?.tls ?? "none");
  let ftpPassive = $state(existing?.ftp?.passive ?? true);
  let s3Region = $state(existing?.s3?.region ?? "");
  let s3Bucket = $state(existing?.s3?.bucket ?? "");
  let s3PathStyle = $state(existing?.s3?.path_style ?? false);
  let s3PublicBaseUrl = $state(existing?.s3?.public_base_url ?? "");
  let s3UploadAcl = $state<S3UploadAcl>(existing?.s3?.upload_acl ?? "private");
  let remoteDir = $state(existing?.remote_dir ?? "");
  let localDir = $state(existing?.local_dir ?? "");
  let tunnels = $state<TunnelConfig[]>(structuredClone($state.snapshot(existing?.tunnels ?? [])) as TunnelConfig[]);
  let disableTerminal = $state(existing?.disable_terminal ?? false);
  let notes = $state(existing?.notes ?? "");
  let saving = $state(false);
  let error = $state<string | null>(null);
  let secretLoadState = $state<"loading" | "ready" | "error">(
    existing ? "loading" : "ready",
  );
  let secretLoadError = $state<string | null>(null);
  let secretLoadAttempt = $state(0);

  // When editing, load the real stored secrets so the form shows them.
  $effect(() => {
    const connection = existing;
    secretLoadAttempt;
    password = "";
    keyPassphrase = "";
    keyInline = "";

    if (!connection) {
      secretLoadState = "ready";
      secretLoadError = null;
      return;
    }

    let cancelled = false;
    secretLoadState = "loading";
    secretLoadError = null;
    void unwrap(commands.connectionSecrets(connection.id))
      .then((secrets) => {
        if (cancelled) return;
        password = secrets.password ?? "";
        keyPassphrase = secrets.key_passphrase ?? "";
        keyInline = secrets.key_inline ?? "";
        secretLoadState = "ready";
      })
      .catch((loadError) => {
        if (cancelled) return;
        secretLoadState = "error";
        secretLoadError = `Could not load saved credentials: ${errorMessage(loadError)}`;
      });

    return () => {
      cancelled = true;
    };
  });

  function retrySecretLoad() {
    if (secretLoadState === "error") secretLoadAttempt += 1;
  }

  const canSave = $derived(
    name.trim() !== "" &&
      host.trim() !== "" &&
      username.trim() !== "" &&
      secretLoadState === "ready" &&
      !saving,
  );

  async function save() {
    if (!canSave) return;
    saving = true;
    error = null;
    const input = buildConnectionInput({
      name,
      badge,
      protocol,
      host,
      port,
      authMethod,
      username,
      password,
      keyPath,
      keySource,
      keyInline,
      keyPassphrase,
      jumpHost,
      ftpTls,
      ftpPassive,
      s3Region,
      s3Bucket,
      s3PathStyle,
      s3PublicBaseUrl,
      s3UploadAcl,
      remoteDir,
      localDir,
      tunnels: $state.snapshot(tunnels) as TunnelConfig[],
      disableTerminal,
      notes,
    });
    try {
      await vault.upsertConnection(existing?.id ?? null, input, parentFolder);
      onclose();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={existing ? `Edit ${existing.name}` : "New connection"} width={520} {onclose}>
  <form
    class="form connection-dialog-form"
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <EndpointFields
      existing={existing !== null}
      bind:name
      bind:protocol
      bind:host
      bind:port
      bind:portTouched
      bind:s3Region
      bind:s3PathStyle
    />
    <AuthenticationSection
      {protocol}
      bind:authMethod
      bind:username
      bind:password
      bind:keyPath
      bind:keySource
      bind:keyInline
      bind:keyPassphrase
      bind:disableTerminal
    />
    <ProtocolOptionsSection
      {protocol}
      existingId={existing?.id ?? null}
      bind:jumpHost
      bind:ftpTls
      bind:ftpPassive
      bind:s3Region
      bind:s3Bucket
      bind:s3PathStyle
      bind:s3PublicBaseUrl
      bind:s3UploadAcl
    />
    <DirectoryFields bind:remoteDir bind:localDir />
    {#if protocol === "ssh"}
      <TunnelsSection bind:tunnels />
    {/if}
    <MetadataFields bind:badge bind:notes />

    {#if secretLoadState === "loading"}
      <div class="hint" aria-live="polite">Loading saved credentials…</div>
    {:else if secretLoadError}
      <div class="credential-error" role="alert">
        <span>{secretLoadError}</span>
        <button type="button" onclick={retrySecretLoad}>Retry</button>
      </div>
    {/if}

    {#if error}
      <div class="error">{error}</div>
    {/if}
  </form>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" disabled={!canSave} onclick={() => void save()}>
      {saving ? "Saving…" : existing ? "Save" : "Create"}
    </button>
  {/snippet}
</Modal>
