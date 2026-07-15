<script lang="ts">
  interface Props {
    appVersion: string;
    onopenrepo: () => void;
  }

  let { appVersion, onopenrepo }: Props = $props();
</script>

<fieldset>
  <legend>About</legend>
  <div class="about">
    <span class="app-name">Serverus{appVersion ? ` v${appVersion}` : ""}</span>
    <button type="button" class="repo-link" onclick={onopenrepo}>
      github.com/fedorananin/serverus
    </button>
  </div>
  <div class="about-note">Free &amp; open source · MIT license</div>
</fieldset>
