<script lang="ts">
	/**
	 * Sign in (SECURITY.md, "Logins"): the owner key or a token, traded once for an HttpOnly
	 * cookie; this browser keeps no key. Asked up front when the server has a key and this
	 * browser is not signed in, and again whenever the server refuses a request.
	 */
	import { onMount } from 'svelte';
	import { auth, adoptStoredKey, LoginRefused } from '#lib/api/auth.js';
	import { nextReason, type PromptReason } from '#lib/auth/prompt.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { KeyRound } from '@lucide/svelte';

	let open = $state(false);
	let key = $state('');
	let error = $state('');
	let busy = $state(false);
	let reason = $state<PromptReason>('missing');
	/** Reads need a credential here: after signing in the page loads again. */
	let readsClosed = $state(false);

	function show(why: PromptReason) {
		reason = nextReason(open, reason, why);
		error = '';
		open = true;
	}

	onMount(() => {
		// While an older dashboard's stored key is traded for a login, refusals are expected.
		let settling = true;
		let refusedMeanwhile = false;
		const onAuthRequired = () => {
			if (settling) refusedMeanwhile = true;
			else show('rejected');
		};
		window.addEventListener('auth_required', onAuthRequired);

		(async () => {
			const adopted = await adoptStoredKey();
			settling = false;
			// Requests refused before the key became a login: load the page again, signed in.
			if (adopted && refusedMeanwhile) return location.reload();
			try {
				const s = await auth.status();
				readsClosed = !s.reads_open;
				if (s.auth_enabled && s.credential === 'none') show('missing');
			} catch {
				/* the server is away: the pages say so */
			}
		})();

		return () => window.removeEventListener('auth_required', onAuthRequired);
	});

	async function signIn() {
		const trimmed = key.trim();
		if (!trimmed) return;
		busy = true;
		error = '';
		try {
			const s = await auth.login(trimmed);
			key = '';
			open = false;
			if (!s.reads_open && reason === 'missing') return location.reload();
			notifications.add(
				s.credential === 'token'
					? `Signed in with the token “${s.token?.name}”.`
					: 'Signed in. Retry the last action.',
				'success'
			);
		} catch (e) {
			error =
				e instanceof LoginRefused
					? e.message || 'Not the owner key or a valid token.'
					: `Could not sign in: ${e instanceof Error ? e.message : e}`;
		} finally {
			busy = false;
		}
	}
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
		<form
			onsubmit={(e) => {
				e.preventDefault();
				void signIn();
			}}
			class="w-full max-w-sm rounded-lg border border-border bg-card p-5 shadow-xl"
		>
			<div class="mb-3 flex items-center gap-2">
				<KeyRound class="h-5 w-5 text-amber-400" />
				<h2 class="text-base font-semibold text-foreground">Sign in</h2>
			</div>
			<p class="mb-3 text-sm text-muted-foreground">
				{#if reason === 'rejected'}
					The server refused the request: sign this browser in.
				{:else if readsClosed}
					This server shows its roasts only to a signed-in browser.
				{:else}
					This server needs a key to control a roaster or change anything.
				{/if}
				Enter the owner key (<code class="rounded bg-background px-1">ROASTWARDEN_API_KEY</code>, or
				the one the server made at its first start:
				<code class="rounded bg-background px-1">owner.key</code>
				beside the database) or an
				<code class="rounded bg-background px-1">rr_…</code> token. The browser keeps a sign-in for 30
				days from its last use, not the key.
			</p>
			<input
				type="password"
				bind:value={key}
				placeholder="Owner key or rr_… token"
				autocomplete="current-password"
				class="mb-2 w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground"
			/>
			{#if error}<p class="mb-2 text-sm text-destructive" role="alert">{error}</p>{/if}
			<div class="mt-1 flex justify-end gap-2">
				<button
					type="button"
					onclick={() => (open = false)}
					class="rounded-md border border-border px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent"
				>
					Later
				</button>
				<button
					type="submit"
					disabled={!key.trim() || busy}
					class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					Sign in
				</button>
			</div>
		</form>
	</div>
{/if}
