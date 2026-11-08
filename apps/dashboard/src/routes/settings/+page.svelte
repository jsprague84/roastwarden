<script lang="ts">
	/** Settings kept in this browser: units (temperature, weight), theme, the heater line, alarm notifications, its sign-in. */
	import { auth, LoginRefused, type AuthStatus } from '#lib/api/auth.js';
	import { resolve } from '$app/paths';
	import { unitState, setUnit, weightState, setWeightUnit } from '#lib/stores/units.svelte.js';
	import { themeState, setTheme } from '#lib/stores/theme.svelte.js';
	import { chartPrefs, setHeaterLine } from '#lib/stores/chartPrefs.svelte.js';
	import { HEATER_SMOOTH_SEC } from '#lib/scope/smooth.js';
	import AlarmNotifications from '#lib/components/settings/AlarmNotifications.svelte';
	import SignedInBrowsers from '#lib/components/settings/SignedInBrowsers.svelte';

	let status = $state<AuthStatus | null>(null);
	let credential = $state('');
	let error = $state('');
	let busy = $state(false);

	async function loadStatus() {
		try {
			status = await auth.status();
		} catch {
			status = null;
		}
	}
	$effect(() => {
		void loadStatus();
	});

	const who = $derived.by(() => {
		if (!status) return '';
		if (!status.auth_enabled)
			return 'The server has no owner key: anyone who reaches it may do everything.';
		if (status.credential === 'demo')
			return 'A shared demo: roast on the simulated roaster and record what you like. Settings, roasters and tokens stay as they are.';
		if (status.credential === 'owner login') return 'Signed in as the owner.';
		if (status.credential === 'token' && status.token)
			return `Signed in with the token “${status.token.name}”.`;
		return status.reads_open
			? 'Not signed in: you can follow roasts, not change anything.'
			: 'Not signed in.';
	});

	async function signIn(e: SubmitEvent) {
		e.preventDefault();
		if (!credential.trim()) return;
		busy = true;
		error = '';
		try {
			status = await auth.login(credential.trim());
			credential = '';
		} catch (err) {
			error =
				err instanceof LoginRefused
					? err.message || 'Not the owner key or a valid token.'
					: `Could not sign in: ${err instanceof Error ? err.message : err}`;
		} finally {
			busy = false;
		}
	}

	async function signOut() {
		busy = true;
		try {
			await auth.logout();
		} catch {
			/* signed out or not, ask again */
		} finally {
			busy = false;
			await loadStatus();
		}
	}

	const seg = 'px-4 py-1.5 text-sm font-medium transition-colors';
</script>

<p class="text-sm text-muted-foreground">
	Kept in this browser only; other screens choose their own.
</p>

<div class="mt-4 divide-y divide-border border-y border-border">
	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Temperature unit</h2>
		<p class="mt-1 text-sm text-muted-foreground">
			The roaster, profiles and roasts are stored in °C; this changes what is shown and how your
			inputs are read.
		</p>
		<div
			class="mt-3 inline-flex overflow-hidden rounded-md border border-border"
			role="group"
			aria-label="Temperature unit"
		>
			{#each [['C', 'Celsius'], ['F', 'Fahrenheit']] as const as [u, label] (u)}
				<button
					type="button"
					class="{seg} {unitState.unit === u
						? 'bg-action text-action-foreground'
						: 'text-foreground hover:bg-accent'}"
					aria-pressed={unitState.unit === u}
					onclick={() => setUnit(u)}
				>
					°{u} · {label}
				</button>
			{/each}
		</div>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Weight unit</h2>
		<p class="mt-1 text-sm text-muted-foreground">
			Stored in grams. US shows ounces, then pounds from 1 lb, and a bare number in a weight field
			is ounces; "250 g" or "2 lb" is read in either. Labels print both.
		</p>
		<div
			class="mt-3 inline-flex overflow-hidden rounded-md border border-border"
			role="group"
			aria-label="Weight unit"
		>
			{#each [['metric', 'Metric · g, kg'], ['us', 'US · oz, lb']] as const as [u, label] (u)}
				<button
					type="button"
					class="{seg} {weightState.unit === u
						? 'bg-action text-action-foreground'
						: 'text-foreground hover:bg-accent'}"
					aria-pressed={weightState.unit === u}
					onclick={() => setWeightUnit(u)}
				>
					{label}
				</button>
			{/each}
		</div>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Theme</h2>
		<div
			class="mt-3 inline-flex overflow-hidden rounded-md border border-border"
			role="group"
			aria-label="Colour theme"
		>
			{#each [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']] as const as [t, label] (t)}
				<button
					type="button"
					class="{seg} {themeState.preference === t
						? 'bg-action text-action-foreground'
						: 'text-foreground hover:bg-accent'}"
					aria-pressed={themeState.preference === t}
					onclick={() => setTheme(t)}
				>
					{label}
				</button>
			{/each}
		</div>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Heater on charts</h2>
		<p class="mt-1 text-sm text-muted-foreground">
			The PID moves the heater every second, so its line jumps by 10–20 points and can crowd the
			scope. Smoothed draws a
			{HEATER_SMOOTH_SEC} s average instead: live, of the last {HEATER_SMOOTH_SEC} s (a few seconds behind);
			on a finished roast, around each moment. Only the drawing changes: the roaster, alarms and stored
			roasts keep every value.
		</p>
		<div
			class="mt-3 inline-flex overflow-hidden rounded-md border border-border"
			role="group"
			aria-label="Heater on charts"
		>
			{#each [['actual', 'Actual'], ['smoothed', `Smoothed (${HEATER_SMOOTH_SEC} s)`]] as const as [v, label] (v)}
				<button
					type="button"
					class="{seg} {chartPrefs.heater === v
						? 'bg-action text-action-foreground'
						: 'text-foreground hover:bg-accent'}"
					aria-pressed={chartPrefs.heater === v}
					onclick={() => setHeaterLine(v)}
				>
					{label}
				</button>
			{/each}
		</div>
	</section>

	<section class="py-5">
		<AlarmNotifications />
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Sign-in</h2>
		<p class="mt-1 text-sm text-muted-foreground">
			{who}
			The owner key may do everything; an API token (<a
				href={resolve('/settings/tokens')}
				class="underline">Tokens</a
			>) may do what it was made for. This browser keeps a sign-in for 30 days from its last use,
			never the key.
		</p>
		{#if status?.auth_enabled}
			{#if status.login_id != null}
				<button
					onclick={signOut}
					disabled={busy}
					class="mt-3 rounded-md border border-border px-4 py-2 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
				>
					Sign out
				</button>
			{:else}
				<form class="mt-3 flex max-w-lg flex-wrap gap-2" onsubmit={signIn}>
					<input
						type="password"
						bind:value={credential}
						placeholder="Owner key or rr_… token"
						autocomplete="current-password"
						class="min-w-0 flex-1 rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
					/>
					<button
						type="submit"
						disabled={!credential.trim() || busy}
						class="rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
						>Sign in</button
					>
				</form>
				{#if error}<p class="mt-2 text-sm text-destructive" role="alert">{error}</p>{/if}
			{/if}
			{#if status.credential === 'owner login'}<SignedInBrowsers mine={status.login_id} />{/if}
		{/if}
	</section>
</div>
