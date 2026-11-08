<script lang="ts">
	/**
	 * Audit (SECURITY.md): every request that changed something, refused ones included, with
	 * who sent it. Newest first; filter by kind, roaster, token and outcome, or search. The
	 * server filters, so "Older" pages through the matches.
	 */
	import { audit } from '#lib/api/audit.js';
	import { describe, who, clientName, AUDIT_KINDS, type AuditEntry } from '#lib/audit/describe.js';
	import { notifications } from '#lib/stores/notifications.js';

	const PAGE = 100;
	let rows = $state<AuditEntry[]>([]);
	let more = $state(false);
	let loading = $state(false);
	let device = $state('');
	let kinds = $state<string[]>([]);
	let token = $state('');
	let failed = $state(false);
	let search = $state('');
	/** `search` once typing pauses */
	let q = $state('');
	let open = $state<number | null>(null);
	// Every roaster and token name seen, so the filters keep their options while they narrow the rows
	let devices = $state<string[]>([]);
	let tokenNames = $state<string[]>([]);
	let seq = 0;

	$effect(() => {
		const text = search;
		const t = setTimeout(() => (q = text.trim()), 300);
		return () => clearTimeout(t);
	});

	async function load(reset: boolean) {
		loading = true;
		const mine = ++seq;
		try {
			const before = reset ? undefined : rows.at(-1)?.id;
			const page = await audit.list({
				limit: PAGE,
				before,
				device_id: device || undefined,
				kinds,
				q,
				token: token || undefined,
				failed
			});
			if (mine !== seq) return; // a newer filter's answer wins
			rows = reset ? page : [...rows, ...page];
			more = page.length === PAGE;
			devices = seen(
				devices,
				page.map((r) => r.device_id)
			);
			tokenNames = seen(
				tokenNames,
				page.map((r) => r.token_name)
			);
		} catch (e) {
			notifications.add(
				`Could not read the audit log: ${e instanceof Error ? e.message : e}`,
				'error'
			);
		} finally {
			loading = false;
		}
	}
	$effect(() => {
		void [device, kinds, token, failed, q];
		void load(true);
	});

	function seen(known: string[], found: (string | null)[]): string[] {
		const add = found.filter((v): v is string => !!v && !known.includes(v));
		return add.length ? [...new Set([...known, ...add])].sort() : known;
	}

	function toggle(kind: string) {
		kinds = kinds.includes(kind) ? kinds.filter((k) => k !== kind) : [...kinds, kind];
	}
	const filtered = $derived(kinds.length > 0 || !!device || !!token || failed || !!q);
	function clear() {
		kinds = [];
		device = '';
		token = '';
		failed = false;
		search = '';
		q = '';
	}

	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'medium' });
	const refused = (s: number) => s === 401 || s === 403;
	const select =
		'rounded-md border border-border bg-input px-3 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
	const chip = 'rounded-sm border px-2 py-0.5 text-xs font-medium';
</script>

<p class="text-sm text-muted-foreground">
	Every request that changed something: control commands, roasts, profiles, settings. Refused ones
	too. Reads are not logged; entries are kept a year.
</p>

<div class="mt-4 flex flex-wrap gap-1.5" role="group" aria-label="Kinds">
	{#each AUDIT_KINDS as k (k.id)}
		<button
			type="button"
			class="{chip} {kinds.includes(k.id)
				? 'border-selected bg-selected text-selected-foreground'
				: 'border-border text-muted-foreground hover:bg-accent hover:text-foreground'}"
			aria-pressed={kinds.includes(k.id)}
			onclick={() => toggle(k.id)}>{k.label}</button
		>
	{/each}
</div>

<div class="mt-3 flex flex-wrap items-center gap-3">
	<input
		type="search"
		bind:value={search}
		placeholder="Search path, body, token, address…"
		aria-label="Search the audit log"
		class="{select} w-full sm:w-72"
	/>
	<label class="text-sm text-muted-foreground"
		>Roaster
		<select bind:value={device} class="{select} ml-1">
			<option value="">All</option>
			{#each devices as d (d)}<option value={d}>{d}</option>{/each}
			{#if device && !devices.includes(device)}<option value={device}>{device}</option>{/if}
		</select>
	</label>
	{#if tokenNames.length}
		<label class="text-sm text-muted-foreground"
			>Token
			<select bind:value={token} class="{select} ml-1">
				<option value="">Any</option>
				{#each tokenNames as t (t)}<option value={t}>{t}</option>{/each}
			</select>
		</label>
	{/if}
	<label class="flex items-center gap-1.5 text-sm text-muted-foreground"
		><input type="checkbox" bind:checked={failed} class="h-4 w-4" /> Refused or failed only</label
	>
	<button class={secondary} onclick={() => load(true)} disabled={loading}>Refresh</button>
	{#if filtered}<button
			class="text-sm text-muted-foreground underline underline-offset-2 hover:text-foreground"
			onclick={clear}>Clear filters</button
		>{/if}
</div>

{#if rows.length}
	<ul class="mt-3 divide-y divide-border border-y border-border text-sm">
		{#each rows as r (r.id)}
			<li class="py-2">
				<button
					class="flex w-full flex-wrap items-baseline gap-x-3 gap-y-0.5 text-left"
					onclick={() => (open = open === r.id ? null : r.id)}
					aria-expanded={open === r.id}
				>
					<span
						class="w-full shrink-0 text-xs tabular-nums text-muted-foreground sm:w-44 sm:text-sm"
						>{when(r.at)}</span
					>
					<span
						class="min-w-0 flex-1 font-medium [overflow-wrap:anywhere] {refused(r.status)
							? 'text-red-400'
							: r.status >= 400
								? 'text-amber-400'
								: 'text-foreground'}"
					>
						{describe(r)}{#if r.status >= 400}<span class="ml-2 text-xs font-normal"
								>{refused(r.status) ? 'refused' : 'failed'} ({r.status})</span
							>{/if}
					</span>
					{#if r.device_id}<span class="text-xs text-muted-foreground">{r.device_id}</span>{/if}
					<span class="text-xs text-muted-foreground"
						>{who(r)}{r.client ? ` · ${clientName(r.client)}` : ''}</span
					>
				</button>
				{#if open === r.id}
					<dl
						class="mt-2 grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-1 border-l-2 border-border pl-3 text-xs"
					>
						<dt class="text-muted-foreground">Request</dt>
						<dd class="break-all font-mono text-foreground">{r.method} {r.path}</dd>
						<dt class="text-muted-foreground">Result</dt>
						<dd class="text-foreground">{r.status}</dd>
						{#if r.body}<dt class="text-muted-foreground">Body</dt>
							<dd class="break-all font-mono text-foreground">{r.body}</dd>{/if}
						<dt class="text-muted-foreground">Credential</dt>
						<dd class="text-foreground">{r.actor}{r.token_name ? ` (${r.token_name})` : ''}</dd>
						{#if r.remote_user}<dt class="text-muted-foreground">Signed in</dt>
							<dd class="text-foreground">{r.remote_user}</dd>{/if}
						{#if r.ip}<dt class="text-muted-foreground">Address</dt>
							<dd class="text-foreground">{r.ip}</dd>{/if}
						{#if r.client}<dt class="text-muted-foreground">Client</dt>
							<dd class="break-all text-foreground">{r.client}</dd>{/if}
					</dl>
				{/if}
			</li>
		{/each}
	</ul>
	{#if more}<button class="{secondary} mt-3" onclick={() => load(false)} disabled={loading}
			>Older</button
		>{/if}
{:else if !loading}
	<p class="mt-3 text-sm text-muted-foreground">
		{filtered ? 'Nothing matches these filters.' : 'Nothing logged yet.'}
	</p>
{/if}
