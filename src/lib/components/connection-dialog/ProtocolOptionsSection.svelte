<script lang="ts">
  import type { FtpTlsMode, Protocol, S3UploadAcl } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import ChoiceRadioGroup from "../ChoiceRadioGroup.svelte";

  const uploadAclOptions = [
    { value: "private", label: "Private" },
    { value: "public_read", label: "Public (public-read)" },
    { value: "ask", label: "Ask before upload" },
  ] as const;

  interface Props {
    protocol: Protocol;
    existingId: string | null;
    jumpHost: string | null;
    ftpTls: FtpTlsMode;
    ftpPassive: boolean;
    s3Region: string;
    s3Bucket: string;
    s3PathStyle: boolean;
    s3PublicBaseUrl: string;
    s3UploadAcl: S3UploadAcl;
  }

  let {
    protocol,
    existingId,
    jumpHost = $bindable(),
    ftpTls = $bindable(),
    ftpPassive = $bindable(),
    s3Region = $bindable(),
    s3Bucket = $bindable(),
    s3PathStyle = $bindable(),
    s3PublicBaseUrl = $bindable(),
    s3UploadAcl = $bindable(),
  }: Props = $props();

  const sshCandidates = $derived(
    Object.values(vault.data?.connections ?? {}).filter(
      (connection) => connection.protocol === "ssh" && connection.id !== existingId,
    ),
  );
</script>

{#if protocol === "ssh"}
  <label>
    <span>Jump host (bastion)</span>
    <select bind:value={jumpHost}>
      <option value={null}>None — direct connection</option>
      {#each sshCandidates as connection (connection.id)}
        <option value={connection.id}>{connection.name} ({connection.host})</option>
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
        <input
          type="text"
          aria-label="S3 region"
          bind:value={s3Region}
          placeholder="fra1"
          class="mono region"
        />
      </label>
    </div>
    <div class="row">
      <ChoiceRadioGroup
        label="Upload files as"
        ariaLabel="S3 upload access"
        name="s3-upload-access"
        value={s3UploadAcl}
        options={uploadAclOptions}
        grow
        onchange={(value) => (s3UploadAcl = value)}
      />
      <label class="checkbox">
        <input type="checkbox" aria-label="S3 path-style URLs" bind:checked={s3PathStyle} />
        <span>Path-style URLs (MinIO)</span>
      </label>
    </div>
    <label>
      <span>Public base URL — CDN / custom domain for “Copy public URL” (optional)</span>
      <input
        type="text"
        aria-label="S3 public base URL"
        bind:value={s3PublicBaseUrl}
        placeholder="https://cdn.example.com"
        class="mono"
      />
    </label>
  </fieldset>
{/if}
