<script lang="ts">
  import type {
    AuthMethod,
    Badge,
    ConnectionInput,
    FtpTlsMode,
    Protocol,
    PublicConnection,
    S3UploadAcl,
    TunnelConfig,
  } from "$lib/api";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import Modal from "./Modal.svelte";
  import BadgePicker from "./BadgePicker.svelte";

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
  // Secrets are shown in cleartext — the vault is already unlocked, so
  // masking adds nothing and makes copying a password between servers harder.
  let showSecrets = $state(true);

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

  // Default port follows protocol until the user edits it.
  $effect(() => {
    if (!portTouched) port = protocol === "ssh" ? 22 : protocol === "ftp" ? 21 : 443;
  });

  // Endpoint templates for popular S3-compatible providers — a convenience
  // fill-in, not a stored setting (an S3 connection is just endpoint + keys).
  const s3Presets: Record<string, { host: string; region: string; pathStyle: boolean }> = {
    do: { host: "fra1.digitaloceanspaces.com", region: "fra1", pathStyle: false },
    aws: { host: "s3.eu-central-1.amazonaws.com", region: "eu-central-1", pathStyle: false },
    r2: { host: "<account-id>.r2.cloudflarestorage.com", region: "auto", pathStyle: false },
    b2: { host: "s3.eu-central-003.backblazeb2.com", region: "eu-central-003", pathStyle: false },
    wasabi: { host: "s3.eu-central-1.wasabisys.com", region: "eu-central-1", pathStyle: false },
    minio: { host: "minio.example.com", region: "us-east-1", pathStyle: true },
  };

  function applyS3Preset(e: Event) {
    const key = (e.currentTarget as HTMLSelectElement).value;
    const preset = s3Presets[key];
    if (!preset) return;
    host = preset.host;
    s3Region = preset.region;
    s3PathStyle = preset.pathStyle;
    (e.currentTarget as HTMLSelectElement).value = "";
  }

  /** Native open panel for the private key, starting in ~/.ssh. */
  async function pickKeyFile() {
    const home = await unwrap(commands.localHome()).catch(() => null);
    const picked = await openFileDialog({
      multiple: false,
      directory: false,
      title: "Choose SSH private key",
      defaultPath: home ? `${home}/.ssh` : undefined,
    });
    if (typeof picked === "string") keyPath = picked;
  }

  let keyImportError = $state<string | null>(null);

  /** Read a key file and store its text in the vault instead — the key then
   *  survives backups and machine moves, unlike a path to a file on disk. */
  async function importKeyFile(path: string) {
    keyImportError = null;
    try {
      keyInline = await unwrap(commands.sshKeyReadFile(path));
      keySource = "text";
      keyPath = "";
    } catch (e) {
      keyImportError = errorMessage(e);
    }
  }

  async function pickKeyToImport() {
    keyImportError = null;
    const home = await unwrap(commands.localHome()).catch(() => null);
    const picked = await openFileDialog({
      multiple: false,
      directory: false,
      title: "Import SSH private key into the vault",
      defaultPath: home ? `${home}/.ssh` : undefined,
    });
    if (typeof picked === "string") await importKeyFile(picked);
  }

  const sshCandidates = $derived(
    Object.values(vault.data?.connections ?? {}).filter(
      (c) => c.protocol === "ssh" && c.id !== existing?.id,
    ),
  );

  const canSave = $derived(
    name.trim() !== "" &&
      host.trim() !== "" &&
      username.trim() !== "" &&
      secretLoadState === "ready" &&
      !saving,
  );

  function addTunnel() {
    tunnels.push({
      name: "",
      kind: "local",
      local_port: 8080,
      remote_host: "127.0.0.1",
      remote_port: 8080,
      autostart: false,
    });
  }

  async function save() {
    if (!canSave) return;
    saving = true;
    error = null;
    const input: ConnectionInput = {
      name: name.trim(),
      badge,
      protocol,
      host: host.trim(),
      port,
      auth_method: authMethod,
      username: username.trim(),
      // The form shows real secrets (vault is unlocked), so it's WYSIWYG:
      // send the fields verbatim — empty clears, a value sets. For key auth
      // the two sources are exclusive: picking one clears the other.
      password,
      key_path:
        protocol === "ssh" && authMethod === "key" && keySource === "text"
          ? null
          : keyPath.trim() === ""
            ? null
            : keyPath.trim(),
      key_inline:
        protocol === "ssh" && authMethod === "key"
          ? keySource === "text"
            ? keyInline
            : ""
          : null,
      key_passphrase: keyPassphrase,
      jump_host: protocol === "ssh" ? jumpHost : null,
      ftp: protocol === "ftp" ? { tls: ftpTls, passive: ftpPassive } : null,
      s3:
        protocol === "s3"
          ? {
              region: s3Region.trim() === "" ? null : s3Region.trim(),
              bucket: s3Bucket.trim() === "" ? null : s3Bucket.trim(),
              path_style: s3PathStyle,
              public_base_url: s3PublicBaseUrl.trim() === "" ? null : s3PublicBaseUrl.trim(),
              upload_acl: s3UploadAcl,
            }
          : null,
      remote_dir: remoteDir.trim() === "" ? null : remoteDir.trim(),
      local_dir: localDir.trim() === "" ? null : localDir.trim(),
      tunnels: protocol === "ssh" ? $state.snapshot(tunnels) : [],
      disable_terminal: protocol === "ssh" ? disableTerminal : false,
      notes,
    };
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
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <div class="row">
      <label class="grow">
        <span>Name</span>
        <input type="text" bind:value={name} placeholder="prod-web-1" />
      </label>
      <label>
        <span>Protocol</span>
        <select bind:value={protocol} disabled={existing !== null}>
          <option value="ssh">SSH / SFTP</option>
          <option value="ftp">FTP / FTPS</option>
          <option value="s3">S3</option>
        </select>
      </label>
    </div>

    <div class="row">
      <label class="grow">
        <span>{protocol === "s3" ? "Endpoint" : "Host"}</span>
        <input
          type="text"
          bind:value={host}
          placeholder={protocol === "s3" ? "fra1.digitaloceanspaces.com" : "server.example.com"}
          class="mono"
        />
      </label>
      {#if protocol === "s3"}
        <label>
          <span>Preset</span>
          <select onchange={applyS3Preset}>
            <option value="">Provider…</option>
            <option value="do">DigitalOcean Spaces</option>
            <option value="aws">AWS S3</option>
            <option value="r2">Cloudflare R2</option>
            <option value="b2">Backblaze B2</option>
            <option value="wasabi">Wasabi</option>
            <option value="minio">MinIO / custom</option>
          </select>
        </label>
      {/if}
      <label class="port">
        <span>Port</span>
        <input
          type="number"
          bind:value={port}
          min="1"
          max="65535"
          oninput={() => (portTouched = true)}
        />
      </label>
    </div>

    <fieldset>
      <legend>Authentication</legend>
      <div class="row">
        <label class="grow">
          <span>{protocol === "s3" ? "Access Key ID" : "Username"}</span>
          <input
            type="text"
            bind:value={username}
            placeholder={protocol === "s3" ? "DO00XXXXXXXXXXXXXXXX" : "root"}
            class="mono"
          />
        </label>
        {#if protocol === "ssh"}
          <label>
            <span>Method</span>
            <select bind:value={authMethod}>
              <option value="password">Password</option>
              <option value="key">Key file</option>
              <option value="agent">SSH agent</option>
            </select>
          </label>
        {/if}
      </div>

      {#if authMethod === "password" || protocol === "ftp" || protocol === "s3"}
        <label>
          <span class="secret-label">
            {protocol === "s3" ? "Secret Access Key" : "Password"}
            <button type="button" class="reveal" onclick={() => (showSecrets = !showSecrets)}>
              {showSecrets ? "hide" : "show"}
            </button>
          </span>
          <input type={showSecrets ? "text" : "password"} class="mono" bind:value={password} />
        </label>
      {/if}

      {#if protocol === "ssh" && authMethod === "key"}
        <label>
          <span>Private key source</span>
          <select bind:value={keySource}>
            <option value="file">Key file on disk</option>
            <option value="text">Key text stored in the vault</option>
          </select>
        </label>
        {#if keySource === "file"}
          <label>
            <span>Private key path</span>
            <div class="key-row">
              <input
                type="text"
                bind:value={keyPath}
                placeholder="~/.ssh/id_ed25519"
                class="mono"
              />
              <button type="button" onclick={() => void pickKeyFile()}>Browse…</button>
            </div>
          </label>
          <div class="key-import">
            <button
              type="button"
              onclick={() =>
                void (keyPath.trim() !== "" ? importKeyFile(keyPath.trim()) : pickKeyToImport())}
            >
              Import into vault as text
            </button>
            <span class="hint">
              A file path breaks on another computer; imported text is encrypted in the vault
              and travels with backups.
            </span>
          </div>
        {:else}
          <label>
            <span class="secret-label">
              Private key (encrypted inside the vault, travels with backups)
              <button type="button" class="reveal" onclick={() => void pickKeyToImport()}>
                import from file…
              </button>
            </span>
            <textarea
              rows="5"
              class="mono key-text"
              bind:value={keyInline}
              placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
              spellcheck="false"
            ></textarea>
          </label>
        {/if}
        {#if keyImportError}
          <div class="error">{keyImportError}</div>
        {/if}
        <label>
          <span>Key passphrase</span>
          <input
            type={showSecrets ? "text" : "password"}
            class="mono"
            bind:value={keyPassphrase}
            placeholder="leave empty if none"
          />
        </label>
      {/if}

      {#if protocol === "ssh"}
        <label class="checkbox">
          <input type="checkbox" bind:checked={disableTerminal} />
          <span>SFTP only — no terminal (server has no shell for this account)</span>
        </label>
      {/if}
    </fieldset>

    {#if protocol === "ssh"}
      <label>
        <span>Jump host (bastion)</span>
        <select bind:value={jumpHost}>
          <option value={null}>None — direct connection</option>
          {#each sshCandidates as c (c.id)}
            <option value={c.id}>{c.name} ({c.host})</option>
          {/each}
        </select>
      </label>
    {:else if protocol === "ftp"}
      <div class="row">
        <label class="grow">
          <span>TLS</span>
          <select bind:value={ftpTls}>
            <option value="none">Plain FTP</option>
            <option value="explicit">FTPS required (AUTH TLS)</option>
          </select>
        </label>
        <label class="checkbox">
          <input type="checkbox" bind:checked={ftpPassive} />
          <span>Passive mode</span>
        </label>
      </div>
    {:else}
      <fieldset>
        <legend>S3</legend>
        <div class="row">
          <label class="grow">
            <span>Bucket (empty = list all buckets)</span>
            <input type="text" bind:value={s3Bucket} placeholder="my-space" class="mono" />
          </label>
          <label>
            <span>Region</span>
            <input type="text" bind:value={s3Region} placeholder="fra1" class="mono region" />
          </label>
        </div>
        <div class="row">
          <label class="grow">
            <span>Upload files as</span>
            <select bind:value={s3UploadAcl}>
              <option value="private">Private</option>
              <option value="public_read">Public (public-read)</option>
              <option value="ask">Ask before upload</option>
            </select>
          </label>
          <label class="checkbox">
            <input type="checkbox" bind:checked={s3PathStyle} />
            <span>Path-style URLs (MinIO)</span>
          </label>
        </div>
        <label>
          <span>Public base URL — CDN / custom domain for “Copy public URL” (optional)</span>
          <input
            type="text"
            bind:value={s3PublicBaseUrl}
            placeholder="https://cdn.example.com"
            class="mono"
          />
        </label>
      </fieldset>
    {/if}

    <div class="row">
      <label class="grow">
        <span>Remote start dir</span>
        <input type="text" bind:value={remoteDir} placeholder="/var/www" class="mono" />
      </label>
      <label class="grow">
        <span>Local start dir</span>
        <input type="text" bind:value={localDir} placeholder="~/Projects" class="mono" />
      </label>
    </div>

    {#if protocol === "ssh"}
      <fieldset>
        <legend>Port tunnels</legend>
        {#each tunnels as tunnel, i (i)}
          <div class="tunnel mono">
            <input type="text" bind:value={tunnel.name} placeholder="name" class="t-name" />
            <input type="number" bind:value={tunnel.local_port} min="1" max="65535" title="Local port" />
            <span class="arrow">→</span>
            <input type="text" bind:value={tunnel.remote_host} placeholder="127.0.0.1" class="t-host" />
            <span>:</span>
            <input type="number" bind:value={tunnel.remote_port} min="1" max="65535" title="Remote port" />
            <label class="checkbox small" title="Start with connection">
              <input type="checkbox" bind:checked={tunnel.autostart} />
              <span>auto</span>
            </label>
            <button type="button" class="remove" onclick={() => tunnels.splice(i, 1)} aria-label="Remove tunnel">✕</button>
          </div>
        {/each}
        <button type="button" class="add" onclick={addTunnel}>+ Add tunnel</button>
      </fieldset>
    {/if}

    <label>
      <span>Badge</span>
      <BadgePicker value={badge} onchange={(b) => (badge = b)} />
    </label>

    <label>
      <span>Notes</span>
      <textarea rows="2" bind:value={notes}></textarea>
    </label>

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

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: flex-end;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label > span {
    font-size: 11px;
    color: var(--text-1);
  }

  .grow {
    flex: 1;
  }

  .port {
    width: 84px;
  }

  .region {
    width: 110px;
  }

  .key-row {
    display: flex;
    gap: 6px;
  }

  .key-row input {
    flex: 1;
    min-width: 0;
  }

  .key-text {
    resize: vertical;
    font-size: 11px;
    white-space: pre;
  }

  .key-import {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .key-import .hint {
    font-size: 10px;
    color: var(--text-2);
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
  }

  legend {
    font-size: 11px;
    color: var(--text-1);
    padding: 0 4px;
  }

  .checkbox {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    padding-bottom: 7px;
  }

  .secret-label {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .reveal {
    background: transparent;
    border: none;
    color: var(--accent);
    font-size: 10px;
    padding: 0;
  }

  .checkbox.small {
    padding-bottom: 0;
    font-size: 11px;
  }

  .tunnel {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .tunnel input[type="number"] {
    width: 68px;
  }

  .t-name {
    width: 80px;
  }

  .t-host {
    flex: 1;
    min-width: 70px;
  }

  .arrow {
    color: var(--text-2);
  }

  .remove {
    background: transparent;
    border: none;
    color: var(--text-2);
    padding: 2px 4px;
  }

  .remove:hover {
    color: var(--danger);
  }

  .add {
    align-self: flex-start;
    font-size: 12px;
    padding: 3px 10px;
  }

  .error {
    color: var(--danger);
    font-size: 12px;
  }

  .hint {
    color: var(--text-2);
    font-size: 12px;
  }

  .credential-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--danger);
    font-size: 12px;
  }

  textarea {
    resize: vertical;
  }
</style>
