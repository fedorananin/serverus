<script lang="ts">
  import type { AuthMethod, Protocol } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import ChoiceRadioGroup from "../ChoiceRadioGroup.svelte";

  const authOptions = [
    { value: "password", label: "Password" },
    { value: "key", label: "Key file" },
    { value: "agent", label: "SSH agent" },
  ] as const;

  interface Props {
    protocol: Protocol;
    authMethod: AuthMethod;
    username: string;
    password: string;
    keyPath: string;
    keySource: "file" | "text";
    keyInline: string;
    keyPassphrase: string;
    disableTerminal: boolean;
  }

  let {
    protocol,
    authMethod = $bindable(),
    username = $bindable(),
    password = $bindable(),
    keyPath = $bindable(),
    keySource = $bindable(),
    keyInline = $bindable(),
    keyPassphrase = $bindable(),
    disableTerminal = $bindable(),
  }: Props = $props();

  let showSecrets = $state(true);
  let keyImportError = $state<string | null>(null);

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

  async function importKeyFile(path: string) {
    keyImportError = null;
    try {
      keyInline = await unwrap(commands.sshKeyReadFile(path));
      keySource = "text";
      keyPath = "";
    } catch (error) {
      keyImportError = errorMessage(error);
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
</script>

<fieldset>
  <legend>Authentication</legend>
  <div class="row">
    <label class="grow">
      <span>{protocol === "s3" ? "Access Key ID" : "Username"}</span>
      <input
        type="text"
        aria-label="Connection username"
        bind:value={username}
        placeholder={protocol === "s3" ? "DO00XXXXXXXXXXXXXXXX" : "root"}
        class="mono"
      />
    </label>
    {#if protocol === "ssh"}
      <ChoiceRadioGroup
        label="Method"
        ariaLabel="SSH authentication method"
        name="ssh-auth-method"
        value={authMethod}
        options={authOptions}
        onchange={(value) => (authMethod = value)}
      />
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
      <input
        type={showSecrets ? "text" : "password"}
        aria-label="Connection password"
        class="mono"
        bind:value={password}
      />
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
            aria-label="SSH private key path"
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
          A file path breaks on another computer; imported text is encrypted in the vault and
          travels with backups.
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
