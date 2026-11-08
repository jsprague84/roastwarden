<script lang="ts">
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { goto, refreshAll } from '$app/navigation';
	import { profiles as profilesApi, type RoastProfile } from '#lib/api/profiles.js';
	import { sessions } from '#lib/api/sessions.js';
	import { outsideColumns } from '#lib/guidance/summary.js';
	import { deviceId } from '#lib/stores/telemetry.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { COLUMNS, filterSessions, sortSessions, toCsv, type ColKey } from '#lib/roast/ranking.js';
	import { PURPOSES, parseFlags, flagLabel } from '#lib/roast/properties.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';
	import type { RoastSession } from '#lib/api/sessions.js';
	import { kept, keep } from '#lib/stores/kept.js';

	let { data }: PageProps = $props();
	// The loader's list, again when it re-runs (e.g. after an import); a delete filters it here.
	let sessionList = $derived(data.sessions);
	let selectedIds = $state<Set<string>>(new Set());

	// --- Artisan log import (one or many .alog files) ---
	let importing = $state(false);
	let importProgress = $state('');
	let fileInput = $state<HTMLInputElement | null>(null);

	async function importFiles(files: FileList | null) {
		if (!files || files.length === 0) return;
		importing = true;
		let ok = 0;
		const failures: string[] = [];
		try {
			for (let i = 0; i < files.length; i++) {
				const f = files[i];
				importProgress = `${i + 1}/${files.length}: ${f.name}`;
				try {
					const text = await f.text();
					await sessions.importArtisan(text, $deviceId ?? undefined, undefined);
					ok++;
				} catch (err) {
					const msg = err instanceof Error ? err.message : String(err);
					failures.push(`${f.name}: ${msg}`);
				}
			}
			if (ok > 0)
				notifications.add(`Imported ${ok} roast${ok === 1 ? '' : 's'} from Artisan`, 'success');
			for (const f of failures) notifications.add(f, 'error', 10000);
			await refreshAll();
		} finally {
			importing = false;
			importProgress = '';
			if (fileInput) fileInput.value = '';
		}
	}

	function formatDate(iso: string): string {
		return new Date(iso).toLocaleDateString(undefined, {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}

	function formatDuration(seconds: number | null): string {
		if (seconds == null) return '--';
		const m = Math.floor(seconds / 60);
		const s = seconds % 60;
		return `${m}:${s.toString().padStart(2, '0')}`;
	}

	// Roast guidance (docs/roast-guidance.md): an amber dot after a value outside its
	// profile's expected range.
	let profileList = $state<RoastProfile[]>([]);
	$effect(() => {
		profilesApi
			.list()
			.then((l) => (profileList = l))
			.catch(() => (profileList = []));
	});
	const outside = $derived(
		new Map(
			(sessionList as RoastSession[]).map(
				(s) =>
					[
						s.id,
						outsideColumns(
							s,
							profileList.find((p) => p.id === s.profile_id)
						)
					] as const
			)
		)
	);
	const OUTSIDE_TITLE: Record<string, string> = {
		loss: 'Weight loss outside what the profile’s roast level expects',
		colorGround: 'Ground colour outside the profile’s roast level',
		dtr: 'Development time outside what the profile’s roast level expects'
	};

	/** A roast's status in words: "active" is the one being roasted now. */
	const statusWord = (st: string) => (st === 'active' ? 'roasting' : st);

	// --- Ranking: filter, sort, CSV ---
	// Kept for this tab: back from a roast, the table is as it was
	const was = kept('history.table', {
		purpose: '',
		search: '',
		completedOnly: false,
		sortKey: 'date' as ColKey,
		sortDir: 'desc' as 'asc' | 'desc'
	});
	let purpose = $state(was.purpose);
	let search = $state(was.search);
	let completedOnly = $state(was.completedOnly);
	let sortKey = $state<ColKey>(was.sortKey);
	let sortDir = $state<'asc' | 'desc'>(was.sortDir);
	$effect(() => keep('history.table', { purpose, search, completedOnly, sortKey, sortDir }));
	const rows = $derived(
		sortSessions(
			filterSessions(sessionList as RoastSession[], { purpose, text: search, completedOnly }),
			sortKey,
			sortDir
		)
	);

	function sortBy(k: ColKey) {
		if (sortKey === k) sortDir = sortDir === 'asc' ? 'desc' : 'asc';
		else {
			sortKey = k;
			sortDir = k === 'name' || k === 'bean' || k === 'purpose' ? 'asc' : 'desc';
		}
	}

	function exportCsv() {
		const blob = new Blob([toCsv(rows)], { type: 'text/csv' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `roastwarden-sessions-${new Date().toISOString().slice(0, 10)}.csv`;
		a.click();
		URL.revokeObjectURL(url);
	}

	function cell(s: RoastSession, k: ColKey): string {
		const v = COLUMNS.find((c) => c.key === k)!.value(s);
		if (v == null) return '–';
		switch (k) {
			case 'date':
				return formatDate(String(v));
			case 'duration':
			case 'fc':
				return formatDuration(Number(v));
			case 'maxBt':
				return fmtTemp(Number(v), 0);
			case 'rating':
				return '★'.repeat(Number(v));
			default:
				return String(v);
		}
	}

	async function deleteSession(e: MouseEvent, id: string) {
		e.stopPropagation();
		if (!confirm('Delete this session? This cannot be undone.')) return;
		try {
			await sessions.delete(id);
			sessionList = sessionList.filter((s: { id: string }) => s.id !== id);
			notifications.add('Session deleted', 'success');
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			notifications.add(`Failed to delete session: ${msg}`, 'error');
		}
	}
</script>

<svelte:head>
	<title>History | Roastwarden</title>
</svelte:head>

<div>
	<div class="flex flex-wrap items-start justify-between gap-3">
		<div>
			<h1 class="text-2xl font-bold text-foreground">History</h1>
			<p class="mt-1 text-sm text-muted-foreground">
				Every roast: open one for its report, tick two or more to compare.
			</p>
		</div>
		<div class="flex items-center gap-2">
			<input
				bind:this={fileInput}
				type="file"
				accept=".alog,.json"
				multiple
				class="hidden"
				onchange={(e) => importFiles((e.target as HTMLInputElement).files)}
			/>
			<button
				onclick={() => fileInput?.click()}
				disabled={importing}
				class="rounded-md border border-border px-4 py-2 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
				title="Import recorded Artisan roasts (.alog) as completed sessions{$deviceId
					? ` for ${$deviceId}`
					: ''}"
			>
				{importing ? `Importing ${importProgress}` : 'Import .alog'}
			</button>
			{#if selectedIds.size >= 2}
				<button
					onclick={() => goto(resolve(`/history/compare?ids=${[...selectedIds].join(',')}`))}
					class="rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover"
				>
					Compare ({selectedIds.size})
				</button>
			{/if}
		</div>
	</div>

	{#if sessionList.length === 0}
		<div class="mt-8 text-center">
			<p class="text-muted-foreground">No roast sessions yet.</p>
			<p class="mt-1 text-sm text-muted-foreground">
				Start one on the Roast page, or import your Artisan .alog history with the button above.
			</p>
		</div>
	{:else}
		<div class="mt-4 flex flex-wrap items-center gap-2 text-sm">
			<select
				bind:value={purpose}
				class="rounded-md border border-border bg-input px-2 py-1 text-foreground"
				aria-label="Roasted for"
			>
				<option value="">All roasts</option>
				{#each PURPOSES as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
			</select>
			<input
				bind:value={search}
				placeholder="Search #, name, bean, notes"
				class="min-w-0 flex-1 rounded-md border border-border bg-input px-2 py-1 text-foreground sm:max-w-xs"
			/>
			<label class="flex items-center gap-1.5 text-muted-foreground"
				><input type="checkbox" bind:checked={completedOnly} class="h-4 w-4" /> Completed only</label
			>
			<span class="ml-auto text-xs text-muted-foreground"
				>{rows.length} of {sessionList.length}</span
			>
			<button
				onclick={exportCsv}
				disabled={!rows.length}
				class="rounded-md border border-border px-3 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-40"
				title="The rows shown, every column; temperatures in °C, times in seconds"
				>Export CSV</button
			>
		</div>
		<!-- Phone: one card per roast -->
		<div class="mt-2 space-y-2 sm:hidden">
			{#each rows as session (session.id)}
				<div class="flex items-start gap-3 rounded-lg border border-border bg-card p-3">
					<input
						type="checkbox"
						checked={selectedIds.has(session.id)}
						onchange={() => {
							const next = new Set(selectedIds);
							if (next.has(session.id)) next.delete(session.id);
							else next.add(session.id);
							selectedIds = next;
						}}
						class="mt-1 h-4 w-4 rounded border-border"
						aria-label="Select for compare"
					/>
					<a href={resolve('/history/[id]', { id: session.id })} class="min-w-0 flex-1">
						<div class="flex items-center gap-2">
							<span class="min-w-0 flex-1 truncate font-medium text-foreground"
								>{#if session.batch_number}<span class="mr-1 tabular-nums text-muted-foreground"
										>#{session.batch_number}</span
									>{/if}{session.name}</span
							>
							{#if session.status !== 'completed'}<span
									class="rounded-full bg-amber-500/15 px-2 py-0.5 text-[11px] text-amber-400"
									>{statusWord(session.status)}</span
								>{/if}
						</div>
						<div class="text-xs text-muted-foreground">
							{cell(session, 'date')}{#if cell(session, 'bean') !== '–'}
								· {cell(session, 'bean')}{/if}
						</div>
						<div class="mt-1 flex flex-wrap gap-x-3 text-xs tabular-nums text-muted-foreground">
							<span>Time <span class="text-foreground">{cell(session, 'duration')}</span></span>
							<span>DTR <span class="text-foreground">{cell(session, 'dtr')}</span></span>
							<span>Max BT <span class="text-foreground">{cell(session, 'maxBt')}</span></span>
							{#if session.rating}<span class="text-amber-400">{cell(session, 'rating')}</span>{/if}
						</div>
					</a>
				</div>
			{/each}
		</div>

		<div class="mt-2 hidden overflow-x-auto rounded-lg border border-border bg-card sm:block">
			<table class="min-w-full divide-y divide-border text-sm">
				<thead class="bg-muted">
					<tr>
						<th class="w-10 px-3 py-2"></th>
						{#each COLUMNS as c (c.key)}
							<th
								class="whitespace-nowrap px-3 py-2 text-xs font-medium text-muted-foreground {c.numeric
									? 'text-right'
									: 'text-left'}"
								title={c.title}
							>
								<button
									onclick={() => sortBy(c.key)}
									class="hover:text-foreground {sortKey === c.key ? 'text-foreground' : ''}"
								>
									{c.key === 'maxBt' ? `${c.label} (${tempUnit()})` : c.label}{sortKey === c.key
										? sortDir === 'asc'
											? ' ▲'
											: ' ▼'
										: ''}
								</button>
							</th>
						{/each}
						<th class="w-12 px-3 py-2"></th>
					</tr>
				</thead>
				<tbody class="divide-y divide-border">
					{#each rows as session (session.id)}
						<tr
							class="cursor-pointer hover:bg-accent"
							onclick={() => goto(resolve('/history/[id]', { id: session.id }))}
						>
							<td class="px-3 py-2" onclick={(e) => e.stopPropagation()}>
								<input
									type="checkbox"
									checked={selectedIds.has(session.id)}
									onchange={() => {
										const next = new Set(selectedIds);
										if (next.has(session.id)) next.delete(session.id);
										else next.add(session.id);
										selectedIds = next;
									}}
									class="h-4 w-4 rounded border-border"
									aria-label="Select for compare"
								/>
							</td>
							{#each COLUMNS as c (c.key)}
								<td
									class="whitespace-nowrap px-3 py-2 {c.numeric
										? 'text-right tabular-nums'
										: ''} {c.key === 'name'
										? 'text-foreground'
										: 'text-muted-foreground'} {c.key === 'rating' && session.rating
										? 'text-amber-400'
										: ''} {c.key === 'bean' ? 'max-w-[14rem] truncate' : ''}"
									title={c.key === 'bean' || c.key === 'name' ? cell(session, c.key) : undefined}
								>
									{#if c.key === 'name'}
										<!-- The name gives way; the status and flags stay readable -->
										<span class="flex max-w-[18rem] items-center gap-1">
											<span class="min-w-0 truncate">{cell(session, c.key)}</span>
											{#if session.status !== 'completed'}<span
													class="shrink-0 text-[11px] {session.status === 'active'
														? 'text-amber-400'
														: 'text-muted-foreground'}">{statusWord(session.status)}</span
												>{/if}
											{#each parseFlags(session.roast_flags) as f (f)}<span
													class="shrink-0 rounded bg-red-500/10 px-1 text-[10px] text-red-400"
													>{flagLabel(f)}</span
												>{/each}
										</span>
									{:else}
										{cell(session, c.key)}{#if outside.get(session.id)?.has(c.key as 'loss')}<span
												class="-mr-2.5 ml-1 inline-block h-1.5 w-1.5 rounded-full bg-amber-500 align-middle"
												title={OUTSIDE_TITLE[c.key]}
												aria-label={OUTSIDE_TITLE[c.key]}
											></span>{/if}
									{/if}
								</td>
							{/each}
							<td class="px-3 py-2 text-right">
								<button
									onclick={(e) => deleteSession(e, session.id)}
									class="rounded p-1 text-muted-foreground hover:bg-red-500/15 hover:text-red-400"
									title="Delete session"
									aria-label="Delete session"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="16"
										height="16"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
										><path d="M3 6h18" /><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /><path
											d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"
										/><line x1="10" y1="11" x2="10" y2="17" /><line
											x1="14"
											y1="11"
											x2="14"
											y2="17"
										/></svg
									>
								</button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
