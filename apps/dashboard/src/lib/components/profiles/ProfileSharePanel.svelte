<script lang="ts">
	/** A profile's public link (docs/profiles.md, "Sharing a profile"): a page with its curve
	 *  and targets, and the profile as files another Roastwarden (or Artisan) imports. Its
	 *  link, how often it was opened and downloaded; make one, another, or revoke it. */
	import { profileShare, type ProfileShareLink } from '#lib/api/profileShare.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { onMount } from 'svelte';

	let { profileId }: { profileId: string } = $props();

	let links = $state<ProfileShareLink[]>([]);
	let loaded = $state(false);
	let busy = $state(false);
	let copied = $state(false);

	const current = $derived(links.find((l) => !l.revoked_at));
	const older = $derived(links.filter((l) => l !== current));

	async function load() {
		try {
			links = await profileShare.forProfile(profileId);
		} catch (e) {
			notifications.add(`Share links: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			loaded = true;
		}
	}
	onMount(load);

	async function make(fresh = false) {
		busy = true;
		try {
			await profileShare.make(profileId, fresh);
			await load();
		} catch (e) {
			notifications.add(`Could not share: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			busy = false;
		}
	}

	async function revoke(l: ProfileShareLink) {
		if (
			!confirm(
				'Revoke this link? Its page and files go at once. Copies already imported elsewhere stay there.'
			)
		)
			return;
		busy = true;
		try {
			await profileShare.revoke(l.code);
			await load();
		} catch (e) {
			notifications.add(`Could not revoke: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			busy = false;
		}
	}

	async function copy(url: string) {
		try {
			await navigator.clipboard.writeText(url);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			notifications.add('Copy failed: select the address and copy it by hand.', 'error');
		}
	}

	function when(t?: string | null): string {
		if (!t) return '';
		return new Date(t).toLocaleString(undefined, {
			day: 'numeric',
			month: 'short',
			hour: 'numeric',
			minute: '2-digit'
		});
	}
	const times = (n: number) => (n === 1 ? 'once' : `${n} times`);
	function used(l: ProfileShareLink): string {
		if (l.views === 0 && l.downloads === 0) return 'Not opened yet';
		return `Opened ${times(l.views)}, downloaded ${times(l.downloads)}, last ${when(l.last_used_at)}`;
	}
	const btn =
		'rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50';
</script>

<div class="space-y-3 text-sm" data-testid="profile-share">
	<p class="text-muted-foreground">
		A public page for this profile: its curve, level and purpose, targets and the events it replays,
		with the profile as a file another Roastwarden imports from the link (or Artisan loads as a
		background). Never its roasts or who made it. Anyone with the link can open it, no login; it
		shows the profile as it is now, so later edits show there too.
	</p>
	{#if !loaded}
		<p class="text-muted-foreground">Loading…</p>
	{:else}
		{#if current}
			<div class="flex flex-wrap items-center gap-2">
				<code
					class="min-w-0 flex-1 break-all rounded border border-border bg-input px-2 py-1.5 text-xs text-foreground"
					data-testid="profile-share-url">{current.url}</code
				>
				<button class={btn} onclick={() => copy(current.url)}>{copied ? 'Copied' : 'Copy'}</button>
				<a class={btn} href={current.url} target="_blank" rel="external noopener noreferrer">Open</a
				>
			</div>
			<p class="text-xs text-muted-foreground">
				{used(current)} · made {when(current.created_at)} by {current.created_by}
			</p>
		{/if}
		<div class="flex flex-wrap gap-2">
			{#if current}
				<button
					class={btn}
					disabled={busy}
					onclick={() => make(true)}
					title="Another link, to revoke one person's later">Another link</button
				>
				<button
					class="{btn} text-red-400 hover:bg-red-500/15"
					disabled={busy}
					onclick={() => revoke(current)}>Revoke</button
				>
			{:else}
				<button
					class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
					disabled={busy}
					onclick={() => make()}>Make a share link</button
				>
			{/if}
		</div>
		{#if older.length}
			<details class="text-xs text-muted-foreground">
				<summary class="cursor-pointer">Other links ({older.length})</summary>
				<ul class="mt-2 space-y-1">
					{#each older as l (l.code)}
						<li class="flex flex-wrap items-center gap-2">
							<code class="break-all">{l.url}</code>
							<span>{l.revoked_at ? `revoked ${when(l.revoked_at)}` : used(l)}</span>
							{#if !l.revoked_at}<button class="underline" disabled={busy} onclick={() => revoke(l)}
									>revoke</button
								>{/if}
						</li>
					{/each}
				</ul>
			</details>
		{/if}
	{/if}
</div>
