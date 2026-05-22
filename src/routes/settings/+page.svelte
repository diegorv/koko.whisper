<script lang="ts">
  import { goto } from "$app/navigation";
  import Settings from "$lib/settings/Settings.svelte";
</script>

<Settings onBack={() => goto("/")} />
