<script lang="ts">
	/**
	 * The real-hardware rule (SECURITY.md, docs/control.md "Tokens"): whether API tokens may
	 * change anything for this roaster. Only the owner key can switch it; tokens may always
	 * control the virtual roaster.
	 */
	import { KeyRound } from '@lucide/svelte';
	import { auth, isOwnerCredential } from '#lib/api/auth.js';
	import { tokens, type RoasterTokenAccess } from '#lib/api/tokens.js';

	let { deviceId }: { deviceId: string } = $props();

	let access = $state<RoasterTokenAccess | null>(null);
	let owner = $state(false);
	let error = $state<string | null>(null);
	let saving = $state(false);

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	$effect(() => {
		const id = deviceId;
		access = null;
		error = null;
		void Promise.all([tokens.access(id), auth.status()])
			.then(([a, s]) => {
				access = a;
				owner = isOwnerCredential(s.credential);
			})
			.catch((e) => (error = msg(e)));
	});

	async function set(allowed: boolean) {
		if (
			allowed &&
			!confirm(
				`Let API tokens control ${deviceId}? A roast or admin token could then run its heater.`
			)
		)
			return;
		saving = true;
		error = null;
		try {
			access = await tokens.setAccess(deviceId, allowed);
		} catch (e) {
			error = msg(e);
		} finally {
			saving = false;
		}
	}
</script>

<section
	class="self-start rounded-lg border border-border bg-card p-4"
	aria-labelledby="token-access-title"
>
	<div class="flex items-center gap-2">
		<KeyRound class="h-4 w-4 text-muted-foreground" />
		<h2 id="token-access-title" class="text-sm font-semibold text-foreground">
			Tokens may control this roaster
		</h2>
	</div>
	{#if !access}
		<p class="mt-2 text-xs text-muted-foreground">{error ?? 'Loading…'}</p>
	{:else if access.always}
		<p class="mt-1 text-xs text-muted-foreground">
			Always: this is the virtual roaster, so scripts and agents with a roast or admin token can
			practise on it.
		</p>
	{:else}
		<p class="mt-1 text-xs text-muted-foreground">
			A physical roaster with a heater. While this is off, API tokens may read it but not command
			it, start or change its roasts, or change its tuning; the owner key and this dashboard always
			can.
		</p>
		<label class="mt-3 flex items-center gap-2 text-sm text-foreground">
			<input
				type="checkbox"
				class="h-4 w-4 accent-action"
				checked={access.tokens_may_control}
				disabled={!owner || saving}
				onchange={(e) => set(e.currentTarget.checked)}
			/>
			{access.tokens_may_control ? 'Allowed' : 'Not allowed'}
		</label>
		{#if !owner}<p class="mt-1 text-xs text-muted-foreground">
				Only the owner key can change this.
			</p>{/if}
		{#if error}<p class="mt-1 text-xs text-red-400">{error}</p>{/if}
	{/if}
</section>
