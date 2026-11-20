<script lang="ts">
  interface Props {
    /** `null` until the current choice has loaded. */
    value: boolean | null;
  }

  let { value = $bindable() }: Props = $props();
</script>

<fieldset>
  <legend>Window</legend>
  <label
    class="checkbox"
    title="Off: the window always opens at its standard size and place. On: it comes back where it was when you quit — moved onto the screen and shrunk to fit if that monitor is gone or smaller now."
  >
    <input
      type="checkbox"
      checked={value ?? false}
      disabled={value === null}
      onchange={(event) => (value = event.currentTarget.checked)}
    />
    <span>Remember window size and position</span>
  </label>
</fieldset>
