<script lang="ts">
	/**
	 * Reports (docs/reports.md, docs/ui-structure.md): documents the server makes with Typst
	 * from its own data. Pick a kind and what it is about; the preview is the server's own
	 * pages, the same ones the PDF has.
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { untrack } from 'svelte';
	import { beans as beansApi } from '#lib/api/beans.js';
	import { downloadFile } from '#lib/api/http.js';
	import { profiles as profilesApi } from '#lib/api/profiles.js';
	import { reports, type ReportKind } from '#lib/api/reports.js';
	import { sessions, type RoastSession } from '#lib/api/sessions.js';
	import { settings } from '#lib/api/settings.js';
	import {
		LIST_KINDS,
		compareFileName,
		comparePair,
		compareParams,
		spanParams,
		type Span,
		cuppingFileName,
		cuppingParams,
		datedFileName,
		hasTemperatures,
		newestOf,
		reportError,
		reportFileName,
		reportPath,
		reportQuery,
		reportableRoasts,
		roastLabel,
		traceParams,
		type CuppingChoices,
		type Paper,
		type Units
	} from '#lib/reports/reports.js';
	import { FileDown, Printer } from '@lucide/svelte';
	import { track } from '#lib/stores/demo.svelte.js';

	let kinds = $state<ReportKind[]>([]);
	let coffees = $state<{ id: string; name: string }[]>([]);
	let profileList = $state<{ id: string; name: string }[]>([]);
	let roasts = $state<RoastSession[]>([]);
	let paper = $state<Paper>('letter');
	let units = $state<Units>('C');
	let loaded = $state(false);

	const kindId = $derived(page.url.searchParams.get('kind') ?? 'roast');
	const kind = $derived(kinds.find((k) => k.id === kindId) ?? kinds[0]);
	const roastId = $derived(page.url.searchParams.get('roast') ?? roasts[0]?.id ?? '');
	const roast = $derived(roasts.find((r) => r.id === roastId));

	$effect(() => {
		Promise.all([
			reports.kinds(),
			sessions.list(undefined, 200),
			settings.get().catch(() => ({}) as Record<string, string>),
			beansApi.list(true).catch(() => []),
			profilesApi.list().catch(() => [])
		])
			.then(([k, list, s, bs, ps]) => {
				kinds = k;
				coffees = bs
					.map((b) => ({ id: b.id, name: b.name }))
					.sort((a, b) => a.name.localeCompare(b.name));
				profileList = ps
					.map((p) => ({ id: p.id, name: p.name }))
					.sort((a, b) => a.name.localeCompare(b.name));
				roasts = reportableRoasts(list);
				// A cupping sheet starts with no roast ticked: a blank form of two samples, or
				// the latest cupping session's scores
				if (s['report_paper'] === 'a4') paper = 'a4';
				if (s['report_units'] === 'F') units = 'F';
			})
			.catch((e) => (error = e instanceof Error ? e.message : String(e)))
			.finally(() => (loaded = true));
	});

	function choose(params: Record<string, string>) {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- a query string built for one navigation
		const q = new URLSearchParams(page.url.search);
		for (const [k, v] of Object.entries(params)) q.set(k, v);
		goto(resolve(`/reports?${q}`), { replace: true, reset: false });
	}

	// The cupping sheet's choices (docs/reports.md, "Cupping sheet")
	let cup = $state<CuppingChoices>({ mode: 'blank', roasts: [], samples: 0, perPage: 2, date: '' });
	function toggleRoast(id: string, on: boolean) {
		cup.roasts = on
			? [...cup.roasts.filter((r) => r !== id), id]
			: cup.roasts.filter((r) => r !== id);
	}

	// Green inventory: the report, or a stock-take sheet to weigh against
	let count = $state(false);
	// A period's and purchases' span; the plan's roaster
	let span = $state<Span>('this-month');
	let spanFrom = $state('');
	let spanTo = $state('');
	let planRoaster = $state('');

	// Where the chosen kind's report is, and what else it asks
	// A coffee's or a profile's report: the newest roast's, until another is picked
	const beanId = $derived(
		page.url.searchParams.get('bean') ?? newestOf(roasts, 'bean_id', coffees)
	);
	const profileId = $derived(
		page.url.searchParams.get('profile') ?? newestOf(roasts, 'profile_id', profileList)
	);
	const roasterFilter = $derived(page.url.searchParams.get('roaster') ?? '');
	const roasters = $derived([...new Set(roasts.map((r) => r.device_id))].sort());
	// Two roasts compared: B the newest (or asked for), A the one before it on its roaster
	const pair = $derived(
		comparePair(roasts, page.url.searchParams.get('a'), page.url.searchParams.get('b'))
	);
	const roastA = $derived(roasts.find((r) => r.id === pair.a));
	const roastB = $derived(roasts.find((r) => r.id === pair.b));
	// A trace: a roast back to its green lot, or a coffee to every bag
	const traceOf = $derived(page.url.searchParams.get('of') === 'bean' ? 'bean' : 'roast');
	const traceQuery = $derived(traceParams(traceOf, roastId, beanId));

	const path = $derived(
		kindId === 'roast'
			? roastId
				? reportPath('roast', roastId)
				: ''
			: kindId === 'compare'
				? pair.a && pair.b && pair.a !== pair.b
					? '/api/reports/compare'
					: ''
				: kindId === 'coffee'
					? beanId
						? reportPath('coffee', beanId)
						: ''
					: kindId === 'profile'
						? profileId
							? reportPath('profile', profileId)
							: ''
						: kindId === 'trace'
							? traceQuery
								? '/api/reports/trace'
								: ''
							: (LIST_KINDS[kindId] ?? '')
	);
	const extra = $derived(
		kindId === 'cupping'
			? cuppingParams(cup)
			: kindId === 'trace'
				? traceQuery
				: kindId === 'compare'
					? compareParams(pair.a, pair.b)
					: kindId === 'inventory' && count
						? 'count=true'
						: kindId === 'profile' && roasterFilter
							? `roaster=${encodeURIComponent(roasterFilter)}`
							: kindId === 'period' || kindId === 'purchases'
								? spanParams(span, new Date(), spanFrom, spanTo)
								: kindId === 'plan' && planRoaster
									? `roaster=${encodeURIComponent(planRoaster)}`
									: ''
	);
	const url = (format: 'pdf' | 'svg') =>
		`${path}?${reportQuery(format, { paper, units })}${extra ? `&${extra}` : ''}`;
	const fileName = $derived(
		kindId === 'cupping'
			? cuppingFileName(cup.mode)
			: kindId === 'compare'
				? compareFileName(roastA, roastB)
				: kindId === 'inventory'
					? datedFileName(count ? 'stock-take' : 'green-inventory')
					: kindId === 'shopping'
						? datedFileName('shopping-list')
						: kindId === 'coffee' ||
							  kindId === 'profile' ||
							  (kindId === 'trace' && traceOf === 'bean')
							? `${kindId}-report.pdf`
							: kindId === 'period' || kindId === 'purchases' || kindId === 'plan'
								? datedFileName(kindId === 'plan' ? 'roast-day' : kindId)
								: roast
									? reportFileName(kindId, roast)
									: `${kindId}-report.pdf`
	);

	let preview = $state<string | null>(null);
	let pages = $state<number | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);

	// No picture: a report that failed, or one of another kind, is not left on show
	function clearPreview() {
		const old = untrack(() => preview);
		preview = null;
		pages = null;
		if (old) URL.revokeObjectURL(old);
	}

	// Another kind starts from an empty preview: the last kind's pages are not this one's
	$effect(() => {
		void kindId;
		untrack(() => {
			clearPreview();
			error = null;
		});
	});

	// Redraw the preview when what it shows changes; the last picture stays, dimmed, meanwhile
	$effect(() => {
		const p = path;
		const u = url('svg');
		const k = kindId;
		if (!p) {
			untrack(clearPreview);
			return;
		}
		const ctl = new AbortController();
		const t = setTimeout(() => {
			busy = true;
			reports
				.fetch(u, ctl.signal)
				.then(({ blob, pages: n }) => {
					const old = untrack(() => preview);
					preview = URL.createObjectURL(blob);
					if (old) URL.revokeObjectURL(old);
					pages = n;
					error = null;
				})
				.catch((e) => {
					if (ctl.signal.aborted) return;
					clearPreview();
					error = reportError(k, e instanceof Error ? e.message : String(e));
				})
				.finally(() => {
					if (!ctl.signal.aborted) busy = false;
				});
		}, 120);
		return () => {
			clearTimeout(t);
			ctl.abort();
		};
	});

	$effect(() => () => {
		const old = untrack(() => preview);
		if (old) URL.revokeObjectURL(old);
	});

	let saving = $state(false);
	async function download() {
		if (!path) return;
		saving = true;
		try {
			await downloadFile(url('pdf'), fileName);
			track('Report made', { kind: kindId });
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	// Open the PDF in a new tab to print it: the tab opens at once (a pop-up blocker allows
	// that), the PDF follows
	async function openPdf() {
		if (!path) return;
		const w = window.open('', '_blank');
		try {
			const { blob } = await reports.fetch(url('pdf'));
			track('Report made', { kind: kindId });
			const pdf = URL.createObjectURL(blob);
			if (w) w.location.href = pdf;
			else window.location.href = pdf;
		} catch (e) {
			w?.close();
			error = e instanceof Error ? e.message : String(e);
		}
	}

	const select =
		'mt-1 block rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
</script>

<svelte:head>
	<title>{kind?.title ?? 'Reports'} · Reports | Roastwarden</title>
</svelte:head>

<div class="mx-auto max-w-6xl">
	<div class="flex flex-wrap items-end gap-x-6 gap-y-2 border-b border-border">
		<h1 class="pb-2 text-2xl font-bold text-foreground">Reports</h1>
		<nav class="flex flex-wrap" aria-label="Report kinds">
			{#each kinds as k (k.id)}
				<button
					type="button"
					onclick={() => choose({ kind: k.id })}
					title={k.description}
					aria-current={kind?.id === k.id ? 'page' : undefined}
					class="-mb-px whitespace-nowrap border-b-2 px-2 py-2 text-sm font-medium sm:px-3
					{kind?.id === k.id
						? 'border-action text-foreground'
						: 'border-transparent text-muted-foreground hover:text-foreground'}"
				>
					{k.title}
				</button>
			{/each}
		</nav>
	</div>

	{#if kind}
		<p class="mt-3 max-w-3xl text-sm text-muted-foreground">{kind.description}</p>
	{/if}

	<div class="mt-4 flex flex-wrap items-end gap-x-4 gap-y-3">
		{#if kindId === 'trace'}
			<label class="min-w-0 text-sm text-foreground">
				Trace
				<select
					class="{select} w-64 max-w-full"
					value={traceOf}
					onchange={(e) => choose({ of: (e.currentTarget as HTMLSelectElement).value })}
				>
					<option value="roast">A roast back to its green lot</option>
					<option value="bean">A coffee to every bag</option>
				</select>
			</label>
		{/if}
		{#if kindId === 'roast' || (kindId === 'trace' && traceOf === 'roast')}
			<label class="min-w-0 text-sm text-foreground">
				Roast
				<select
					class="{select} w-80 max-w-full"
					value={roastId}
					onchange={(e) => choose({ roast: (e.currentTarget as HTMLSelectElement).value })}
					disabled={!roasts.length}
				>
					{#each roasts as r (r.id)}
						<option value={r.id}>{roastLabel(r)}</option>
					{/each}
				</select>
			</label>
		{/if}
		{#if kindId === 'compare'}
			{#each [{ key: 'a', label: 'A', hint: 'the roast B is measured against', value: pair.a }, { key: 'b', label: 'B', hint: 'the change', value: pair.b }] as side (side.key)}
				<label class="min-w-0 text-sm text-foreground">
					{side.label} <span class="text-muted-foreground">· {side.hint}</span>
					<select
						class="{select} w-72 max-w-full"
						value={side.value}
						onchange={(e) =>
							choose({
								a: pair.a,
								b: pair.b,
								[side.key]: (e.currentTarget as HTMLSelectElement).value
							})}
						disabled={roasts.length < 2}
					>
						{#each roasts as r (r.id)}
							<option value={r.id}>{roastLabel(r)}</option>
						{/each}
					</select>
				</label>
			{/each}
		{/if}
		{#if kindId === 'coffee' || (kindId === 'trace' && traceOf === 'bean')}
			<label class="min-w-0 text-sm text-foreground">
				Coffee
				<select
					class="{select} w-80 max-w-full"
					value={beanId}
					onchange={(e) => choose({ bean: (e.currentTarget as HTMLSelectElement).value })}
					disabled={!coffees.length}
				>
					{#each coffees as b (b.id)}
						<option value={b.id}>{b.name}</option>
					{/each}
				</select>
			</label>
		{/if}
		{#if kindId === 'profile'}
			<label class="min-w-0 text-sm text-foreground">
				Profile
				<select
					class="{select} w-80 max-w-full"
					value={profileId}
					onchange={(e) => choose({ profile: (e.currentTarget as HTMLSelectElement).value })}
					disabled={!profileList.length}
				>
					{#each profileList as p (p.id)}
						<option value={p.id}>{p.name}</option>
					{/each}
				</select>
			</label>
			<label class="text-sm text-foreground">
				Roasts on
				<select
					class={select}
					value={roasterFilter}
					onchange={(e) => choose({ roaster: (e.currentTarget as HTMLSelectElement).value })}
				>
					<option value="">every roaster</option>
					{#each roasters as d (d)}
						<option value={d}>{d}</option>
					{/each}
				</select>
			</label>
		{/if}
		{#if kindId === 'period' || kindId === 'purchases'}
			<label class="text-sm text-foreground">
				Period
				<select class={select} bind:value={span}>
					<option value="this-month">This month</option>
					<option value="last-month">Last month</option>
					<option value="this-year">This year{kindId === 'period' ? ' (in review)' : ''}</option>
					<option value="last-year">Last year{kindId === 'period' ? ' (in review)' : ''}</option>
					<option value="range">From … to …</option>
				</select>
			</label>
			{#if span === 'range'}
				<label class="text-sm text-foreground"
					>From<input type="date" bind:value={spanFrom} class="{select} w-44" /></label
				>
				<label class="text-sm text-foreground"
					>To<input type="date" bind:value={spanTo} class="{select} w-44" /></label
				>
			{/if}
		{/if}
		{#if kindId === 'plan'}
			<label class="text-sm text-foreground">
				Roaster
				<select class={select} bind:value={planRoaster}>
					<option value="">every roaster</option>
					{#each roasters as d (d)}
						<option value={d}>{d}</option>
					{/each}
				</select>
			</label>
		{/if}
		{#if kindId === 'cupping'}
			<div class="text-sm text-foreground">
				<span id="cup-mode">Sheet</span>
				<div class="mt-1 flex" role="radiogroup" aria-labelledby="cup-mode">
					{#each [{ v: 'blank' as const, label: 'A form to score on' }, { v: 'scores' as const, label: 'Scores' }] as m, i (m.v)}
						<button
							type="button"
							role="radio"
							aria-checked={cup.mode === m.v}
							onclick={() => (cup.mode = m.v)}
							class="border border-border px-3 py-2 text-sm {i === 0
								? 'rounded-l-md'
								: '-ml-px rounded-r-md'}
							{cup.mode === m.v
								? 'border-selected bg-selected text-selected-foreground'
								: 'bg-input text-muted-foreground hover:text-foreground'}"
						>
							{m.label}
						</button>
					{/each}
				</div>
			</div>
			{#if cup.mode === 'blank'}
				<label class="text-sm text-foreground">
					Your own samples
					<input
						type="number"
						min="0"
						max="24"
						step="1"
						bind:value={cup.samples}
						class="{select} w-24"
					/>
				</label>
				<label class="text-sm text-foreground">
					Layout
					<select class={select} bind:value={cup.perPage}>
						<option value={2}>2 a page, scales to mark</option>
						<option value={6}>6 a page, boxes</option>
					</select>
				</label>
			{:else if !cup.roasts.length}
				<label class="text-sm text-foreground">
					Day <span class="text-muted-foreground">(empty: the latest)</span>
					<input type="date" bind:value={cup.date} class="{select} w-44" />
				</label>
			{/if}
		{/if}
		<label class="text-sm text-foreground">
			Paper
			<select class={select} bind:value={paper}>
				<option value="letter">US Letter</option>
				<option value="a4">A4</option>
			</select>
		</label>
		{#if kindId === 'inventory'}
			<div class="text-sm text-foreground">
				<span id="inv-mode">Page</span>
				<div class="mt-1 flex" role="radiogroup" aria-labelledby="inv-mode">
					{#each [{ v: false, label: 'Stock and value' }, { v: true, label: 'Stock-take sheet' }] as m, i (m.label)}
						<button
							type="button"
							role="radio"
							aria-checked={count === m.v}
							onclick={() => (count = m.v)}
							class="border border-border px-3 py-2 text-sm {i === 0
								? 'rounded-l-md'
								: '-ml-px rounded-r-md'}
							{count === m.v
								? 'border-selected bg-selected text-selected-foreground'
								: 'bg-input text-muted-foreground hover:text-foreground'}"
						>
							{m.label}
						</button>
					{/each}
				</div>
			</div>
		{/if}
		{#if hasTemperatures(kindId)}
			<label class="text-sm text-foreground">
				Temperatures
				<select class={select} bind:value={units}>
					<option value="C">°C</option>
					<option value="F">°F</option>
				</select>
			</label>
		{/if}
		<div class="flex items-center gap-2 sm:ml-auto">
			<button
				type="button"
				onclick={download}
				disabled={!path || saving || !preview}
				class="inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
			>
				<FileDown size={16} aria-hidden="true" />{saving ? 'Making…' : 'Download PDF'}
			</button>
			<button
				type="button"
				onclick={openPdf}
				disabled={!path || !preview}
				title="Open the PDF in a new tab to print it"
				class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-2 text-sm text-foreground hover:bg-accent disabled:opacity-50"
			>
				<Printer size={16} aria-hidden="true" />Open to print
			</button>
		</div>
	</div>

	{#if kindId === 'cupping'}
		<fieldset class="mt-3">
			<legend class="text-sm text-foreground">
				Roasts
				<span class="text-muted-foreground">
					{cup.mode === 'blank'
						? '· each gets a blind code; the key prints on its own page'
						: '· their scores; none ticked: the latest cupping session'}
				</span>
			</legend>
			<div
				class="mt-1 grid max-h-48 gap-x-4 gap-y-1 overflow-y-auto rounded-sm border border-border bg-card p-2 sm:grid-cols-2 lg:grid-cols-3"
			>
				{#each roasts.slice(0, 30) as r (r.id)}
					<label class="flex min-w-0 items-center gap-2 text-sm text-foreground">
						<input
							type="checkbox"
							class="accent-action"
							checked={cup.roasts.includes(r.id)}
							onchange={(e) => toggleRoast(r.id, (e.currentTarget as HTMLInputElement).checked)}
						/>
						<span class="truncate">{roastLabel(r)}</span>
					</label>
				{/each}
			</div>
		</fieldset>
	{/if}

	{#if error}
		<p class="mt-3 whitespace-pre-wrap text-sm text-destructive" role="alert">{error}</p>
	{/if}

	<div class="mt-4 rounded-sm border border-border bg-muted/40 p-2 sm:p-6" aria-busy={busy}>
		{#if preview}
			<img
				src={preview}
				alt="{kind?.title ?? 'The report'}{kindId === 'roast' && roast
					? `: ${roastLabel(roast)}`
					: ''}, as it prints"
				class="mx-auto block w-full max-w-[850px] transition-opacity {busy ? 'opacity-60' : ''}"
			/>
			{#if pages}
				<p class="mt-2 text-center text-xs text-muted-foreground">
					{pages}
					{pages === 1 ? 'page' : 'pages'} · as it prints
				</p>
			{/if}
		{:else if loaded && !path}
			<p class="py-16 text-center text-sm text-muted-foreground">
				{kindId === 'roast'
					? 'No finished roast yet: a report comes once a roast is complete.'
					: kindId === 'compare'
						? pair.a && pair.a === pair.b
							? 'A and B are the same roast: pick two.'
							: 'Two finished roasts make a comparison.'
						: 'Nothing to report yet.'}
			</p>
		{:else if error && !busy}
			<p class="py-16 text-center text-sm text-muted-foreground">No report to show: see above.</p>
		{:else}
			<p class="py-16 text-center text-sm text-muted-foreground">Making the report…</p>
		{/if}
	</div>
</div>
