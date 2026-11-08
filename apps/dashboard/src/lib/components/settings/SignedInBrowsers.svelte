<script lang="ts">
	/**
	 * The signed-in browsers (SECURITY.md, "Logins"; the owner's alone): which, as the owner or a
	 * token, when last used, and sign one or all out (a lost phone).
	 */
	import { auth, type Login } from '#lib/api/auth.js';
	import { ago, browserName } from '#lib/tokens/state.js';
	import { notifications } from '#lib/stores/notifications.js';

	let { mine }: { mine: number | null } = $props();
	let logins = $state<Login[]>([]);

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);
	const when = (iso: string) =>
		new Date(iso).toLocaleDateString(undefined, { dateStyle: 'medium' });

	async function load() {
		try {
			logins = await auth.logins();
		} catch (e) {
			notifications.add(`Could not read the signed-in browsers: ${msg(e)}`, 'error');
		}
	}
	$effect(() => {
		void load();
	});

	async function end(l: Login) {
		const self = l.id === mine;
		if (self && !confirm('Sign this browser out?')) return;
		try {
			await auth.endLogin(l.id);
			if (self) return location.reload();
			await load();
		} catch (e) {
			notifications.add(`Could not sign it out: ${msg(e)}`, 'error');
		}
	}

	async function endAll() {
		if (
			!confirm('Sign every browser out, this one too? Each signs in again with the key or a token.')
		)
			return;
		try {
			await auth.endAllLogins();
			location.reload();
		} catch (e) {
			notifications.add(`Could not sign them out: ${msg(e)}`, 'error');
		}
	}
</script>

<div class="mt-4" data-testid="logins">
	<div class="flex items-baseline gap-3">
		<h3 class="text-sm font-medium text-foreground">Signed-in browsers</h3>
		{#if logins.length > 1}
			<button class="ml-auto text-xs text-muted-foreground underline" onclick={endAll}
				>Sign out everywhere</button
			>
		{/if}
	</div>
	<p class="mt-1 text-xs text-muted-foreground">
		A new owner key signs every owner browser out; revoking a token signs out the browsers that used
		it.
	</p>
	{#if logins.length}
		<ul class="mt-1 max-w-2xl divide-y divide-border text-sm">
			{#each logins as l (l.id)}
				<li class="flex items-center gap-3 py-2">
					<div class="min-w-0 flex-1">
						<p class="truncate">
							<span class="text-foreground">{browserName(l.user_agent)}</span>
							{#if l.id === mine}<span class="ml-2 text-xs text-selected">this browser</span>{/if}
						</p>
						<p class="mt-0.5 text-xs text-muted-foreground">
							{l.kind === 'owner' ? 'owner' : `token “${l.token_name}”`} · used {ago(
								l.last_used_at
							)}{l.last_used_ip ? ` from ${l.last_used_ip}` : ''} · signed in {when(l.created_at)} · until
							{when(l.expires_at)}
						</p>
					</div>
					<button
						class="shrink-0 rounded-md border border-border px-3 py-1 text-xs font-medium text-foreground hover:bg-accent"
						onclick={() => end(l)}>Sign out</button
					>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="mt-1 text-sm text-muted-foreground">None.</p>
	{/if}
</div>
