<script lang="ts">
	/**
	 * Tokens (SECURITY.md, docs/control.md "Tokens"): scoped API tokens for scripts and agents.
	 * Only the owner key manages them; the secret is shown once.
	 */
	import { auth, isOwnerCredential, type AuthStatus, type TokenPreset } from '#lib/api/auth.js';
	import { tokens, type ApiToken } from '#lib/api/tokens.js';
	import { listDevices } from '#lib/api/devices.js';
	import { resolve } from '$app/paths';
	import {
		PRESETS,
		presetLabel,
		tokenState,
		ago,
		agentSetup,
		panelSetup
	} from '#lib/tokens/state.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { Copy, Check } from '@lucide/svelte';

	let status = $state<AuthStatus | null>(null);
	let rows = $state<ApiToken[]>([]);
	let name = $state('');
	let preset = $state<TokenPreset>('read_only');
	let expiry = $state('90');
	let askFirst = $state(true);
	/** A panel token's roaster, and the roasters to pick it from */
	let roaster = $state('');
	let roasters = $state<{ device_id: string; name: string }[]>([]);
	let busy = $state(false);
	let secret = $state<{ name: string; value: string; roaster: string | null } | null>(null);
	let copied = $state(false);

	const owner = $derived(isOwnerCredential(status?.credential));
	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	async function load() {
		try {
			status = await auth.status();
			if (isOwnerCredential(status.credential) || status.credential === 'open')
				rows = await tokens.list();
		} catch (e) {
			notifications.add(`Could not read the tokens: ${msg(e)}`, 'error');
		}
	}
	// The roasters a panel can be for; without them the Panel preset says to add one
	$effect(() => {
		listDevices()
			.then((all) => {
				roasters = all.filter((d) => d.status === 'active' || d.status === 'pending');
				if (!roaster && roasters.length) roaster = roasters[0].device_id;
			})
			.catch(() => {});
	});
	$effect(() => {
		void load();
	});

	async function create(e: SubmitEvent) {
		e.preventDefault();
		if (!name.trim() || (preset === 'panel' && !roaster)) return;
		busy = true;
		try {
			const days = expiry === 'never' ? null : Number(expiry);
			const r =
				preset === 'panel'
					? await tokens.createPanel(name.trim(), roaster, days)
					: await tokens.create(name.trim(), preset, days, askFirst);
			secret = { name: r.token.name, value: r.secret, roaster: r.token.roaster };
			copied = false;
			name = '';
			await load();
		} catch (err) {
			notifications.add(`Could not create the token: ${msg(err)}`, 'error');
		} finally {
			busy = false;
		}
	}

	async function copy() {
		if (!secret) return;
		try {
			await navigator.clipboard.writeText(secret.value);
			copied = true;
		} catch {
			notifications.add('Copy failed: select the token and copy it by hand.', 'error');
		}
	}

	async function revoke(t: ApiToken) {
		if (!confirm(`Revoke “${t.name}”? Anything using it stops working at once.`)) return;
		try {
			await tokens.revoke(t.id);
			await load();
		} catch (e) {
			notifications.add(`Could not revoke: ${msg(e)}`, 'error');
		}
	}

	async function setAskFirst(t: ApiToken, on: boolean) {
		try {
			await tokens.setRequireApproval(t.id, on);
			await load();
		} catch (e) {
			notifications.add(`Could not change “${t.name}”: ${msg(e)}`, 'error');
		}
	}

	async function remove(t: ApiToken) {
		try {
			await tokens.remove(t.id);
			await load();
		} catch (e) {
			notifications.add(`Could not remove: ${msg(e)}`, 'error');
		}
	}

	const when = (iso: string) =>
		new Date(iso).toLocaleDateString(undefined, { dateStyle: 'medium' });
	const input =
		'rounded-md border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
	const seg = 'px-3 py-1.5 text-sm font-medium transition-colors';
</script>

<p class="text-sm text-muted-foreground">
	Tokens let scripts and agents use the API with less than the owner key can do. Send one as
	<code class="rounded bg-background px-1 text-xs">Authorization: Bearer rr_…</code>. A token never
	controls a physical roaster unless you allow it on that roaster's Control page. A Panel token is
	the one exception: you make it for one roaster's touch panel, and it works that roaster's panel
	and nothing else.
</p>

{#if status && !status.auth_enabled}
	<p class="mt-4 border-l-2 border-amber-400 pl-3 text-sm text-foreground">
		The API is open: <code class="rounded bg-background px-1 text-xs">ROASTWARDEN_API_KEY</code> is not
		set, so anyone who can reach the server can do everything and a token would restrict nothing. Set
		the key to use tokens.
	</p>
{:else if status && !owner}
	<p class="mt-4 border-l-2 border-amber-400 pl-3 text-sm text-foreground">
		Only the owner manages tokens. This browser is signed in
		{status.credential === 'token' && status.token
			? `with the ${presetLabel(status.token.preset).toLowerCase()} token “${status.token.name}”`
			: 'with nothing'}; sign in with the owner key under
		<a href={resolve('/settings')} class="underline">This browser</a>.
	</p>
{/if}

{#if owner}
	<div class="mt-4 divide-y divide-border border-y border-border">
		<section class="py-5">
			<h2 class="text-sm font-semibold text-foreground">New token</h2>
			<form class="mt-3 space-y-3" onsubmit={create}>
				<div class="flex flex-wrap items-end gap-3">
					<label
						class="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground sm:max-w-xs"
					>
						Name
						<input
							bind:value={name}
							maxlength="60"
							placeholder="e.g. analysis agent"
							class={input}
							required
						/>
					</label>
					<label class="flex flex-col gap-1 text-xs text-muted-foreground">
						Expires
						<select bind:value={expiry} class={input}>
							<option value="30">in 30 days</option>
							<option value="90">in 90 days</option>
							<option value="365">in a year</option>
							<option value="never">never</option>
						</select>
					</label>
				</div>
				<div>
					<div
						class="inline-flex overflow-hidden rounded-md border border-border"
						role="group"
						aria-label="Access"
					>
						{#each PRESETS as p (p.id)}
							<button
								type="button"
								class="{seg} {preset === p.id
									? 'bg-action text-action-foreground'
									: 'text-foreground hover:bg-accent'}"
								aria-pressed={preset === p.id}
								onclick={() => (preset = p.id)}>{p.label}</button
							>
						{/each}
					</div>
					<p class="mt-1.5 text-xs text-muted-foreground">
						{PRESETS.find((p) => p.id === preset)?.line}
					</p>
				</div>
				{#if preset === 'panel'}
					<label class="flex max-w-xs flex-col gap-1 text-xs text-muted-foreground">
						Roaster
						<select bind:value={roaster} class={input} data-testid="panel-roaster">
							{#each roasters as r (r.device_id)}
								<option value={r.device_id}>{r.name} ({r.device_id})</option>
							{/each}
						</select>
					</label>
					<p class="text-xs text-muted-foreground">
						{#if roasters.length}
							A panel never asks first: the person pressing Start stands at the roaster. The
							roaster's control level still decides what is sent.
						{:else}
							Add a roaster first (<a href={resolve('/roasters')} class="underline">Roasters</a>).
						{/if}
					</p>
				{:else}
					<label class="flex items-start gap-2 text-sm text-foreground">
						<input type="checkbox" bind:checked={askFirst} class="mt-0.5" />
						<span
							>Ask me first <span class="text-muted-foreground"
								>before it starts a roast on a physical roaster or deletes a roast, bean, profile or
								roaster (<a href={resolve('/settings/approvals')} class="underline">Approvals</a
								>)</span
							></span
						>
					</label>
				{/if}
				<button
					type="submit"
					disabled={busy || !name.trim() || (preset === 'panel' && !roaster)}
					class="rounded-md bg-action px-4 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					Create token
				</button>
			</form>

			{#if secret}
				<div class="mt-4 border-l-2 border-go pl-3" data-testid="token-secret">
					<p class="text-sm font-medium text-foreground">
						“{secret.name}” created. Copy it now: it is not shown again.
					</p>
					<div class="mt-2 flex max-w-xl items-center gap-2">
						<code
							class="min-w-0 flex-1 select-all break-all rounded bg-background px-2 py-1.5 font-mono text-xs text-foreground"
							>{secret.value}</code
						>
						<button type="button" class={secondary} onclick={copy} aria-label="Copy token">
							{#if copied}<Check class="h-4 w-4" />{:else}<Copy class="h-4 w-4" />{/if}
						</button>
					</div>
					{#if secret.roaster}
						<p class="mt-3 text-sm text-foreground">
							For the panel's sketch, at the top of its settings:
						</p>
						<pre
							class="mt-1 max-w-xl overflow-x-auto rounded bg-background px-2 py-1.5 font-mono text-xs text-foreground"
							data-testid="panel-setup">{panelSetup(
								window.location.origin,
								secret.roaster,
								secret.value
							)}</pre>
						<p class="mt-1 text-xs text-muted-foreground">
							The address is this page's; use the one the panel reaches the server at. Building a
							panel: docs, "Touch panel".
						</p>
					{:else}
						<p class="mt-3 text-sm text-foreground">
							For an AI agent, on its machine (with <code class="font-mono text-xs">roast</code> installed):
						</p>
						<pre
							class="mt-1 max-w-xl overflow-x-auto rounded bg-background px-2 py-1.5 font-mono text-xs text-foreground"
							data-testid="agent-setup">{agentSetup(window.location.origin)}</pre>
						<p class="mt-1 text-xs text-muted-foreground">
							The skill teaches the agent the commands and the rules. Setting up an agent: docs,
							"Agents".
						</p>
					{/if}
					<button
						type="button"
						class="mt-2 text-xs text-muted-foreground underline"
						onclick={() => (secret = null)}>Done</button
					>
				</div>
			{/if}
		</section>

		<section class="py-5">
			<h2 class="text-sm font-semibold text-foreground">Tokens</h2>
			{#if rows.length}
				<ul class="mt-2 divide-y divide-border text-sm">
					{#each rows as t (t.id)}
						{@const st = tokenState(t)}
						<li class="flex items-center gap-3 py-2.5 {st.kind === 'active' ? '' : 'opacity-60'}">
							<div class="min-w-0 flex-1">
								<p class="truncate">
									<span class="font-medium text-foreground">{t.name}</span>
									<span class="ml-2 font-mono text-xs text-muted-foreground">{t.prefix}…</span>
								</p>
								<p class="mt-0.5 text-xs text-muted-foreground">
									{presetLabel(t.preset)}{t.roaster ? ` for ${t.roaster}` : ''}{t.require_approval
										? ' · asks first'
										: ''} ·
									<span class={st.kind === 'active' ? '' : 'text-red-400'}>{st.text}</span>
									· used {ago(t.last_used_at)}{t.last_used_ip ? ` from ${t.last_used_ip}` : ''} · created
									{when(t.created_at)}
								</p>
							</div>
							{#if st.kind === 'active'}
								{#if t.preset !== 'panel'}
									<button
										class="{secondary} shrink-0 py-1 text-xs"
										title={t.require_approval
											? 'Let it start roasts on a physical roaster and delete without asking'
											: 'Ask before it starts a roast on a physical roaster or deletes'}
										onclick={() => setAskFirst(t, !t.require_approval)}
										>{t.require_approval ? 'Stop asking' : 'Ask first'}</button
									>
								{/if}
								<button class="{secondary} shrink-0 py-1 text-xs" onclick={() => revoke(t)}
									>Revoke</button
								>
							{:else}
								<button class="{secondary} shrink-0 py-1 text-xs" onclick={() => remove(t)}
									>Remove</button
								>
							{/if}
						</li>
					{/each}
				</ul>
			{:else}
				<p class="mt-2 text-sm text-muted-foreground">No tokens yet.</p>
			{/if}
		</section>
	</div>
{:else if status?.credential === 'open'}
	{#if rows.length}
		<p class="mt-4 text-sm text-muted-foreground">
			{rows.length} token{rows.length === 1 ? '' : 's'} stored; they work again once the key is set.
		</p>
	{/if}
{/if}
