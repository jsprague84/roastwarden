<script lang="ts">
	/**
	 * Download PDF, Print and "Printed it" for a sheet of labels (docs/labels.md, "Print
	 * runs"), the same on a roast's Labels page and Label Studio → Print. A Download makes a
	 * run (kept in History; the same labels again answer the same run while it is not
	 * printed); "Printed it" marks it printed so the sheet's next free label moves on, here,
	 * without going to History. Under `lg` it sits at the foot of the window, above it at
	 * the top of the preview, so it is in view however long the form.
	 */
	import { labelStudio, type LabelRun } from '#lib/api/labelStudio.js';
	import { resolve } from '$app/paths';
	import { runPlace } from '#lib/labels/runs.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { FileDown, Printer, Check } from '@lucide/svelte';

	let {
		ask,
		disabled = false,
		why = '',
		per,
		printerId = '',
		tray = '',
		color = '',
		loadHint = '',
		onsheets,
		onmade
	}: {
		/** The run's body (`POST /api/label-runs`) */
		ask: () => Record<string, unknown>;
		disabled?: boolean;
		/** Why it is disabled, in words */
		why?: string;
		/** Labels on the sheet */
		per: number;
		printerId?: string;
		tray?: string;
		color?: string;
		/** What to load before a print ("Load a 5163 sheet, labels 1–3 used") */
		loadHint?: string;
		/** The sheets' next free labels changed: reload them */
		onsheets?: () => Promise<void> | void;
		/** A run was made (it may have taken bags off a roast) */
		onmade?: () => Promise<void> | void;
	} = $props();

	// The label printer, when one is set (Settings → Label printers)
	let printer = $state<{
		configured: boolean;
		problem?: string;
		printer?: { make_and_model?: string };
	} | null>(null);
	$effect(() => {
		const id = printerId;
		labelStudio
			.printer(id || undefined)
			.then((p) => (printer = p))
			.catch(() => (printer = null));
	});

	// The last run made here, until it is marked printed or another is made
	let run = $state<LabelRun | null>(null);
	let busy = $state<'' | 'pdf' | 'print' | 'done'>('');
	let said = $state('');

	async function download() {
		busy = 'pdf';
		try {
			const made = await labelStudio.makeRun(ask());
			await labelStudio.runPdf(made.id);
			run = made;
			said = made.reused ? 'The same labels as before: the same run, not a second copy.' : '';
			if (made.packed?.length) said = `${said} ${packedSaid(made.packed)}`.trim();
			await onmade?.();
		} catch (e) {
			notifications.add(`Labels: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			busy = '';
		}
	}

	async function print() {
		busy = 'print';
		try {
			const made = await labelStudio.makeRun(ask());
			await onmade?.();
			const r = await labelStudio.printRun(made.id, {
				printer: printerId || undefined,
				tray: tray || undefined,
				color: color || undefined
			});
			if (r.approval)
				notifications.add(
					'The print waits for the owner’s approval (Settings → Approvals)',
					'warning'
				);
			else
				notifications.add(
					`${r.printer}: ${r.message ?? 'sent'}${r.job_id ? ` (job ${r.job_id})` : ''}`,
					r.job_state === 'completed' ? 'success' : 'warning'
				);
			// A printer that finishes the job moves the sheet on by itself
			run = null;
			said = '';
			await onsheets?.();
		} catch (e) {
			notifications.add(`Print: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			busy = '';
		}
	}

	function packedSaid(packed: { bags?: number | null; bag_g?: number | null }[]): string {
		return `Taken off as packed: ${packed.map((p) => `${p.bags ?? '?'} × ${p.bag_g ?? '?'} g`).join(', ')}.`;
	}

	async function printed() {
		if (!run) return;
		busy = 'done';
		try {
			await labelStudio.runDone(run.id);
			const next = ((run.start - 1 + run.labels) % per) + 1;
			said = `Marked printed: the ${run.sheet} sheet now starts at label ${next}${next === 1 ? ' (a new sheet)' : ''}.`;
			run = null;
			await onsheets?.();
		} catch (e) {
			notifications.add(`Labels: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			busy = '';
		}
	}
</script>

<div
	data-testid="run-actions"
	class="fixed inset-x-0 bottom-0 z-20 border-t border-border bg-card px-3 py-2 shadow-lg lg:sticky lg:top-14 lg:mb-3 lg:rounded-md lg:border lg:shadow-none"
>
	<div class="flex flex-wrap items-center gap-2">
		<button
			onclick={download}
			disabled={disabled || !!busy}
			title={why || 'Kept in Label Studio → History; print it at Actual size'}
			class="inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
		>
			<FileDown class="h-4 w-4" />
			{busy === 'pdf' ? 'Making the PDF…' : 'Download PDF'}
		</button>
		{#if printer?.configured}
			<button
				onclick={print}
				disabled={disabled || !!busy || !!printer.problem}
				title={printer.problem ?? (loadHint || 'Print at actual size on label stock')}
				class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
			>
				<Printer class="h-4 w-4" />
				{busy === 'print'
					? 'Sending…'
					: `Print on ${printer.printer?.make_and_model ?? 'the label printer'}`}
			</button>
		{/if}
		{#if run}
			<button
				onclick={printed}
				disabled={!!busy}
				data-testid="printed-it"
				title="Mark the run printed: the sheet's next free label moves on"
				class="inline-flex items-center gap-1.5 rounded-md border border-go/60 px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
			>
				<Check class="h-4 w-4" />
				{busy === 'done' ? 'Marking…' : 'Printed it'}
			</button>
		{/if}
	</div>
	<p class="mt-1 text-xs text-muted-foreground" aria-live="polite">
		{#if run}
			Printed {runPlace(run, per)}? Mark it so the sheet's next free label moves on.
			<a class="text-action hover:underline" href={resolve(`/labels/history?run=${run.id}`)}
				>In History</a
			>
		{:else if said}
			{said}
		{:else if printer?.problem}
			{printer.problem}
		{:else}
			Print at <strong>Actual size</strong> (not “Fit to page”), paper type “Labels”.
		{/if}
		{#if run && said}<br />{said}{/if}
	</p>
</div>
