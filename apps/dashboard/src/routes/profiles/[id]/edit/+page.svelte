<script lang="ts">
	import ProfileDesigner from '#lib/components/profiles/ProfileDesigner.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head>
	<title>Edit {data.profile.name} | Roastwarden</title>
</svelte:head>

<ProfileDesigner initialProfile={data.profile} />
