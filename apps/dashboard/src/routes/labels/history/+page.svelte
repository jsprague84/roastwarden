<script lang="ts">
	/**
	 * Label Studio → History (docs/labels.md, "Print runs"): what was printed when, on which
	 * sheet from which label, and how the printer's job went, newest first. A run's PDF, a
	 * print of one that was made, "printed it" for a PDF printed by hand (the sheet's next free
	 * label moves on), and a reprint of all or some of its labels, as they were.
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import {
		labelStudio,
		type LabelRun,
		type LabelRunSummary,
		type SheetNext
	} from '#lib/api/labelStudio.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { onMount } from 'svelte';
	import { ChevronDown, ChevronRight } from '@lucide/svelte';

	let runs = $state<LabelRunSummary[]>([]);
	let sheets = $state<SheetNext[]>([]);
	let loading = $state(true);
	let open = $state<LabelRun | null>(null);
	let printer = $state<{ configured: boolean; problem?: string } | null>(null);

	async function load() {
		try {
			[runs, sheets] = await Promise.all([labelStudio.runs(100), labelStudio.sheets()]);
		} catch (e) {
			notifications.add(`History: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			loading = false;
		}
	}
	onMount(async () => {
		await load();
		const asked = page.url.searchParams.get('run');
		if (asked) await show(asked);
		labelStudio
			.printer()
			.then((p) => (printer = p))
			.catch(() => (printer = null));
	});

	// A run whose printer has not finished: look again every few seconds while it is open
	$effect(() => {
		const r = open;
		if (!r || !['sent', 'pending', 'processing', 'stopped', 'held'].includes(r.state)) return;
		const t = setInterval(async () => {
			try {
				const fresh = await labelStudio.run(r.id);
				if (open?.id === r.id) open = fresh;
				if (fresh.state !== r.state) await load();
			} catch {
				/* the next tick tries again */
			}
		}, 4000);
		return () => clearInterval(t);
	});

	async function show(id: string) {
		if (open?.id === id) {
			open = null;
			return;
		}
		try {
			open = await labelStudio.run(id);
			reprintStart = null;
			pick = [];
		} catch (e) {
			notifications.add(`Run: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}

	async function act(what: string, fn: () => Promise<unknown>) {
		try {
			await fn();
			await load();
			if (open) open = await labelStudio.run(open.id);
		} catch (e) {
			notifications.add(`${what}: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}

	async function print(r: LabelRunSummary) {
		await act('Print', async () => {
			const p = await labelStudio.printRun(r.id);
			if (p.approval)
				notifications.add(
					'The print waits for the owner’s approval (Settings → Approvals)',
					'warning'
				);
			else
				notifications.add(
					`${p.printer}: ${p.message}`,
					p.job_state === 'completed' ? 'success' : 'warning'
				);
		});
	}

	// Reprint: all, or the labels ticked, from the sheet's next free label unless set
	let reprintStart = $state<number | null>(null);
	let pick = $state<number[]>([]);
	const nextFor = (sheet: string) => sheets.find((s) => s.sheet === sheet)?.next_label ?? 1;
	const perFor = (sheet: string) => sheets.find((s) => s.sheet === sheet)?.per_sheet ?? 10;
	// Each label of a run, in order: its item's name
	const labelsOf = (r: LabelRun) =>
		r.items.flatMap((it) => Array.from({ length: it.count }, () => it.name));
	async function reprint(r: LabelRun) {
		await act('Reprint', async () => {
			const again = await labelStudio.reprint(r.id, {
				start: reprintStart ?? undefined,
				labels: pick.length ? [...pick].sort((a, b) => a - b) : undefined
			});
			notifications.add(
				`Reprint made: ${again.labels} label${again.labels === 1 ? '' : 's'} from label ${again.start}`,
				'success'
			);
			open = await labelStudio.run(again.id);
		});
	}

	const STATE: Record<string, [string, string]> = {
		made: ['PDF made', 'text-muted-foreground'],
		sent: ['sent', 'text-highlight'],
		pending: ['queued', 'text-highlight'],
		processing: ['printing', 'text-highlight'],
		stopped: ['waiting at the printer', 'text-amber-600 dark:text-amber-400'],
		held: ['held at the printer', 'text-amber-600 dark:text-amber-400'],
		completed: ['printed', 'text-go'],
		printed: ['printed by hand', 'text-go'],
		aborted: ['aborted', 'text-destructive'],
		canceled: ['canceled', 'text-destructive'],
		failed: ['not sent', 'text-destructive']
	};
	const stateText = (s: string) => STATE[s]?.[0] ?? s;
	const stateClass = (s: string) => STATE[s]?.[1] ?? 'text-muted-foreground';
	const when = (iso: string) =>
		new Date(iso).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
	const range = (r: LabelRunSummary) => {
		const per = perFor(r.sheet);
		const end = ((r.start - 1 + r.labels - 1) % per) + 1;
		return r.pages > 1
			? `labels ${r.start}–${per}, then ${r.pages - 1} more sheet${r.pages > 2 ? 's' : ''} to ${end}`
			: r.labels === 1
				? `label ${r.start}`
				: `labels ${r.start}–${end}`;
	};
	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
</script>

<div class="flex flex-wrap items-baseline gap-x-6 gap-y-2">
	<p class="max-w-3xl text-sm text-muted-foreground">
		Every sheet of labels made here, on a roast's Labels page or by an agent, with the PDF as it was
		made. A sheet's next free label moves on when the printer finishes a job, or when you mark a PDF
		printed.
	</p>
	<a
		href={resolve('/labels/print')}
		class="ml-auto rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
		>New print run</a
	>
</div>

{#if sheets.some((s) => s.next_label > 1)}
	<p class="mt-3 text-sm text-foreground">
		Part-used sheets:
		{#each sheets.filter((s) => s.next_label > 1) as s, i (s.sheet)}
			{i ? ' · ' : ''}<span class="tabular-nums"
				>{s.sheet}: next label {s.next_label} of {s.per_sheet}</span
			>
			<button
				class="text-xs text-action hover:underline"
				onclick={() => act('Sheet', () => labelStudio.setNext(s.sheet, 1))}>new sheet</button
			>
		{/each}
	</p>
{/if}

{#if loading}
	<p class="mt-6 text-sm text-muted-foreground">Loading…</p>
{:else if !runs.length}
	<p class="mt-6 text-sm text-muted-foreground">
		Nothing printed yet. <a class="text-action underline" href={resolve('/labels/print')}
			>Make a print run</a
		>, or print from a roast's Labels page.
	</p>
{:else}
	<table class="mt-4 w-full text-sm">
		<thead>
			<tr class="border-b border-border text-left text-xs text-muted-foreground">
				<th class="w-6"></th>
				<th class="hidden py-1.5 pr-3 font-medium sm:table-cell">When</th>
				<th class="py-1.5 pr-3 font-medium">What</th>
				<th class="hidden py-1.5 pr-3 font-medium md:table-cell">Sheet</th>
				<th class="py-1.5 pr-3 font-medium">How it went</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each runs as r (r.id)}
				<tr
					class="cursor-pointer border-b border-border align-top hover:bg-accent/50"
					onclick={() => show(r.id)}
				>
					<td class="py-2 text-muted-foreground"
						>{#if open?.id === r.id}<ChevronDown class="h-4 w-4" />{:else}<ChevronRight
								class="h-4 w-4"
							/>{/if}</td
					>
					<td
						class="hidden whitespace-nowrap py-2 pr-3 tabular-nums text-muted-foreground sm:table-cell"
						>{when(r.created_at)}</td
					>
					<td class="py-2 pr-3">
						<!-- On a phone the date sits above what was printed, which gets the width -->
						<span class="block text-xs tabular-nums text-muted-foreground sm:hidden"
							>{when(r.created_at)}</span
						>
						<span class="text-foreground">{r.summary}</span>
						<span class="block text-xs text-muted-foreground"
							>{r.labels} label{r.labels === 1 ? '' : 's'}{r.reprint_of
								? ' · a reprint'
								: ''}{r.note ? ` · ${r.note}` : ''} · by {r.created_by}</span
						>
					</td>
					<td class="hidden py-2 pr-3 text-muted-foreground md:table-cell">{r.sheet}, {range(r)}</td
					>
					<td class="whitespace-nowrap py-2 pr-3 font-medium {stateClass(r.state)}"
						>{stateText(r.state)}</td
					>
					<td
						class="whitespace-nowrap py-2 text-right text-xs"
						onclick={(e) => e.stopPropagation()}
					>
						<button
							class="text-action hover:underline"
							onclick={() =>
								labelStudio.runPdf(r.id).catch((e) => notifications.add(`PDF: ${e}`, 'error'))}
							>PDF</button
						>
						{#if r.state === 'made'}
							{#if printer?.configured}· <button
									class="text-action hover:underline"
									onclick={() => print(r)}>Print</button
								>{/if}
							·
							<button
								class="text-action hover:underline"
								title="You printed the PDF yourself: the sheet's next free label moves on"
								onclick={() => act('Printed', () => labelStudio.runDone(r.id))}>Printed it</button
							>
						{/if}
					</td>
				</tr>
				{#if open?.id === r.id}
					<tr class="border-b border-border">
						<td></td>
						<td colspan="5" class="pb-4 pt-1">
							<div class="grid gap-5 md:grid-cols-2">
								<div>
									<h3 class="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
										On the sheet
									</h3>
									<ul class="mt-1 space-y-1">
										{#each open.items as it, k (k)}
											<li>
												<span class="text-foreground">{it.name}</span>
												<span class="text-muted-foreground">
													×{it.count} in <span class="font-mono text-xs">{it.design}</span>{it.data
														?.message
														? `, “${it.data.message}”`
														: ''}</span
												>
												{#if it.roast}<a
														class="ml-1 text-xs text-action hover:underline"
														href={resolve('/history/[id]', { id: it.roast })}>roast</a
													>{/if}
											</li>
										{/each}
									</ul>
									{#if open.printer}
										<h3
											class="mt-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground"
										>
											The printer's job {open.job_id ?? ''}
										</h3>
										<ol class="mt-1 space-y-0.5 text-xs">
											{#each open.states as st, k (k)}
												<li>
													<span class="tabular-nums text-muted-foreground">{when(st.at)}</span>
													<span class="font-medium {stateClass(st.state)}"
														>{stateText(st.state)}</span
													>{st.reasons?.length ? ` (${st.reasons.join(', ')})` : ''}{st.sheets
														? ` · ${st.sheets} sheet${st.sheets === 1 ? '' : 's'}`
														: ''}
												</li>
											{/each}
										</ol>
										{#if open.tray}<p class="mt-1 text-xs text-muted-foreground">
												From {open.tray}.
											</p>{/if}
									{/if}
								</div>
								<div>
									<h3 class="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
										Reprint
									</h3>
									<p class="mt-1 text-xs text-muted-foreground">
										The labels as they were (the same design versions, dates and QR codes). Untick
										none for all.
									</p>
									<div class="mt-2 flex flex-wrap gap-1">
										{#each labelsOf(open) as name, k (k)}
											<label
												class="inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-xs {pick.includes(
													k + 1
												)
													? 'bg-selected text-selected-foreground'
													: 'text-foreground'}"
												title={name}
											>
												<input
													type="checkbox"
													class="sr-only"
													checked={pick.includes(k + 1)}
													onchange={() =>
														(pick = pick.includes(k + 1)
															? pick.filter((p) => p !== k + 1)
															: [...pick, k + 1])}
												/>
												{k + 1}
											</label>
										{/each}
									</div>
									<div class="mt-2 flex items-end gap-2">
										<label class="block text-xs font-medium text-muted-foreground"
											>From label
											<input
												class="{field} w-24"
												type="number"
												min="1"
												max={perFor(open.sheet)}
												value={reprintStart ?? nextFor(open.sheet)}
												oninput={(e) => (reprintStart = Number(e.currentTarget.value) || 1)}
											/>
										</label>
										<button
											class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
											onclick={() => reprint(open!)}
										>
											Reprint {pick.length
												? `${pick.length} label${pick.length === 1 ? '' : 's'}`
												: 'all'}
										</button>
									</div>
								</div>
							</div>
						</td>
					</tr>
				{/if}
			{/each}
		</tbody>
	</table>
{/if}
