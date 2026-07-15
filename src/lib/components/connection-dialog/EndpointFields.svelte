<script lang="ts">
  import type { Protocol } from "$lib/api";
  import ChoiceRadioGroup from "../ChoiceRadioGroup.svelte";

  const protocolOptions = [
    { value: "ssh", label: "SSH / SFTP" },
    { value: "ftp", label: "FTP / FTPS" },
    { value: "s3", label: "S3" },
  ] as const;
  const s3Presets: Record<string, { host: string; region: string; pathStyle: boolean }> = {
    do: { host: "fra1.digitaloceanspaces.com", region: "fra1", pathStyle: false },
    aws: { host: "s3.eu-central-1.amazonaws.com", region: "eu-central-1", pathStyle: false },
    r2: { host: "<account-id>.r2.cloudflarestorage.com", region: "auto", pathStyle: false },
    b2: {
      host: "s3.eu-central-003.backblazeb2.com",
      region: "eu-central-003",
      pathStyle: false,
    },
    wasabi: { host: "s3.eu-central-1.wasabisys.com", region: "eu-central-1", pathStyle: false },
    minio: { host: "minio.example.com", region: "us-east-1", pathStyle: true },
  };

  interface Props {
    existing: boolean;
    name: string;
    protocol: Protocol;
    host: string;
    port: number;
    portTouched: boolean;
    s3Region: string;
    s3PathStyle: boolean;
  }

  let {
    existing,
    name = $bindable(),
    protocol = $bindable(),
    host = $bindable(),
    port = $bindable(),
    portTouched = $bindable(),
    s3Region = $bindable(),
    s3PathStyle = $bindable(),
  }: Props = $props();

  $effect(() => {
    if (!portTouched) port = protocol === "ssh" ? 22 : protocol === "ftp" ? 21 : 443;
  });

  function applyS3Preset(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const preset = s3Presets[select.value];
    if (!preset) return;
    host = preset.host;
    s3Region = preset.region;
    s3PathStyle = preset.pathStyle;
    select.value = "";
  }
</script>

<div class="row">
  <label class="grow">
    <span>Name</span>
    <input type="text" aria-label="Connection name" bind:value={name} placeholder="prod-web-1" />
  </label>
  <ChoiceRadioGroup
    label="Protocol"
    ariaLabel="Connection protocol"
    name="connection-protocol"
    value={protocol}
    options={protocolOptions}
    disabled={existing}
    onchange={(value) => (protocol = value)}
  />
</div>

<div class="row">
  <label class="grow">
    <span>{protocol === "s3" ? "Endpoint" : "Host"}</span>
    <input
      type="text"
      aria-label="Connection host"
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
      aria-label="Connection port"
      bind:value={port}
      min="1"
      max="65535"
      oninput={() => (portTouched = true)}
    />
  </label>
</div>
