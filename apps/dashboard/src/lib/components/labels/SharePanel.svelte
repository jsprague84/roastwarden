<script lang="ts">
	/** A roast's public share page (docs/labels.md, "The share page"): what a bag's QR code
	 *  opens with "Share page" chosen on its labels. Its link, how often it was opened, the
	 *  line for the person it is for; make one, another, or revoke it. */
	import { share, type ShareLink } from '#lib/api/share.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { onMount } from 'svelte';

	let { sessionId }: { sessionId: string } = $props();

	let links = $state<ShareLink[]>([]);
	let loaded = $state(false);
	let busy = $state(false);
	let message = $state('');
	let copied = $state(false);

	const current = $derived(links.find((l) => !l.revoked_at));
	const older = $derived(links.filter((l) => l !== current));

	async function load() {
		try {
			links = await share.forRoast(sessionId);
			message = links.find((l) => !l.revoked_at)?.message ?? '';
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
			await share.make(sessionId, { message: message.trim(), fresh });
			await load();
		} catch (e) {
			notifications.add(`Could not share: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			busy = false;
		}
	}

	async function revoke(l: ShareLink) {
		if (!confirm('Revoke this link? Its page goes at once, and bags printed with it open nothing.'))
			return;
		busy = true;
		try {
			await share.revoke(l.code);
			message = '';
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
		const d = new Date(t);
		return d.toLocaleString(undefined, {
			day: 'numeric',
			month: 'short',
			hour: 'numeric',
			minute: '2-digit'
		});
	}
	function opened(l: ShareLink): string {
		if (l.scans === 0) return 'Not opened yet';
		const times = l.scans === 1 ? 'once' : `${l.scans} times`;
		return `Opened ${times}, last ${when(l.last_scan_at)}`;
	}
	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const btn =
		'rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50';
</script>

<div class="space-y-3 text-sm">
	<p class="text-muted-foreground">
		A public page for this roast, for the person holding the bag: the coffee, the roast date and
		when it is at its best, the notes, the curve and a brew suggestion; never its cost or stock.
		Anyone with the link can open it, no login. On labels, <b>QR code: Share page</b>
		opens it.
	</p>
	{#if !loaded}
		<p class="text-muted-foreground">Loading…</p>
	{:else}
		{#if current}
			<div class="flex flex-wrap items-center gap-2">
				<code
					class="min-w-0 flex-1 break-all rounded border border-border bg-input px-2 py-1.5 text-xs text-foreground"
					data-testid="share-url">{current.url}</code
				>
				<button class={btn} onclick={() => copy(current.url)}>{copied ? 'Copied' : 'Copy'}</button>
				<a class={btn} href={current.url} target="_blank" rel="external noopener noreferrer">Open</a
				>
			</div>
			<p class="text-xs text-muted-foreground">
				{opened(current)} · made {when(current.created_at)} by {current.created_by}
			</p>
		{/if}
		<label class="block text-xs font-medium text-muted-foreground">
			A line for the person it is for (on the page)
			<input class={field} bind:value={message} maxlength="280" placeholder="Happy birthday, Sam" />
		</label>
		<div class="flex flex-wrap gap-2">
			{#if current}
				<button
					class={btn}
					disabled={busy || message.trim() === (current.message ?? '')}
					onclick={() => make()}>Save the line</button
				>
				<button
					class={btn}
					disabled={busy}
					onclick={() => make(true)}
					title="Another link, for someone else, with its own line">Another link</button
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
							<span
								>{l.revoked_at ? `revoked ${when(l.revoked_at)}` : opened(l)}{l.message
									? ` · “${l.message}”`
									: ''}</span
							>
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
