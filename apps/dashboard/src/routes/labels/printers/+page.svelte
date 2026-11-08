<script lang="ts">
	/**
	 * Label Studio → Printers (docs/labels.md, "Printers"): the label printers, each asked
	 * how it is, how labels go to it (PDF, or PWG Raster / URF for a printer without PDF), its
	 * tray and colour, and its calibration per sheet from the calibration page. An `ipps`
	 * printer is trusted by its certificate's SHA-256, checked by the owner.
	 */
	import { labelStudio, type FoundPrinter, type LabelPrinter } from '#lib/api/labelStudio.js';
	import { labels as labelsApi } from '#lib/api/labels.js';
	import type { LabelSheet } from '#lib/labels/labels.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { idFrom, validId } from '#lib/labels/studio.js';
	import { ago } from '#lib/tokens/state.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { onMount } from 'svelte';
	import { Plus, RefreshCw, Search, Trash2 } from '@lucide/svelte';

	let printers = $state<LabelPrinter[]>([]);
	let sheets = $state<LabelSheet[]>([]);
	let loading = $state(true);

	async function load() {
		try {
			const [p, o] = await Promise.all([labelStudio.printers(), labelsApi.options()]);
			printers = p;
			sheets = o.sheets;
		} catch (e) {
			notifications.add(`Printers: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			loading = false;
		}
	}
	// Each printer is asked how it is now, as the print pages ask theirs, so a state from an
	// old check is not shown as today's; meanwhile the old one shows with its age
	let asking = $state<string[]>([]);
	async function askAll() {
		asking = printers.map((p) => p.id);
		await Promise.all(
			printers.map(async (p) => {
				try {
					const now = await labelStudio.printerNow(p.id);
					printers = printers.map((x) => (x.id === p.id ? now : x));
				} catch {
					/* the list's answer stays, with its age */
				} finally {
					asking = asking.filter((id) => id !== p.id);
				}
			})
		);
	}
	onMount(() => {
		load().then(askAll);
	});

	const fail = (what: string, e: unknown) =>
		notifications.add(`${what}: ${e instanceof Error ? e.message : String(e)}`, 'error');

	// ---- adding one ----
	let adding = $state(false);
	let form = $state({ id: '', name: '', url: 'ipp://', send: 'auto', fingerprint: '' });
	let idTouched = $state(false);
	let found = $state<FoundPrinter[] | null>(null);
	let findNote = $state('');
	let finding = $state(false);
	let cert = $state('');
	let saving = $state(false);
	$effect(() => {
		if (!idTouched) form.id = idFrom(form.name);
	});
	const secure = $derived(form.url.trim().toLowerCase().startsWith('ipps://'));

	async function find() {
		finding = true;
		try {
			const r = await labelStudio.findPrinters();
			found = r.printers;
			findNote = r.note;
		} catch (e) {
			fail('Find', e);
		} finally {
			finding = false;
		}
	}
	function pick(f: FoundPrinter) {
		form.name = f.model ? `${f.model}` : f.name;
		form.url = f.url;
	}
	async function readCertificate() {
		try {
			cert = (await labelStudio.certificate(form.url.trim())).fingerprint;
		} catch (e) {
			fail('Certificate', e);
		}
	}
	async function add() {
		if (!validId(form.id))
			return notifications.add('Give it an id: letters, digits, - and _', 'error');
		saving = true;
		try {
			const p = await labelStudio.addPrinter({
				id: form.id,
				name: form.name.trim() || form.id,
				url: form.url.trim(),
				send: form.send,
				fingerprint: secure ? form.fingerprint.trim() || null : null
			});
			notifications.add(
				p.problem
					? `Saved ${p.name}, but: ${p.problem}`
					: `Added ${p.name}: ${p.info?.make_and_model ?? ''}`,
				p.problem ? 'warning' : 'success'
			);
			adding = false;
			form = { id: '', name: '', url: 'ipp://', send: 'auto', fingerprint: '' };
			idTouched = false;
			cert = '';
			await load();
		} catch (e) {
			fail('Add', e);
		} finally {
			saving = false;
		}
	}

	// ---- one printer ----
	let checking = $state<string | null>(null);
	async function check(p: LabelPrinter) {
		checking = p.id;
		try {
			const now = await labelStudio.printerNow(p.id);
			printers = printers.map((x) => (x.id === p.id ? now : x));
			notifications.add(
				now.problem ? `${p.name}: ${now.problem}` : `${p.name}: ${now.info?.state ?? 'answered'}`,
				now.problem ? 'warning' : 'success'
			);
		} catch (e) {
			fail('Check', e);
		} finally {
			checking = null;
		}
	}
	async function save(p: LabelPrinter, change: Partial<LabelPrinter>) {
		try {
			const next = await labelStudio.editPrinter(p.id, {
				name: p.name,
				url: p.url,
				tray: p.tray ?? null,
				color: p.color ?? null,
				send: p.send,
				fingerprint: p.fingerprint ?? null,
				is_default: p.is_default,
				...change
			});
			printers = printers.map((x) => (x.id === p.id ? next : x));
		} catch (e) {
			fail('Save', e);
		}
	}
	async function makeDefault(p: LabelPrinter) {
		try {
			await labelStudio.defaultPrinter(p.id);
			await load();
		} catch (e) {
			fail('Default', e);
		}
	}
	async function remove(p: LabelPrinter) {
		if (!confirm(`Delete ${p.name} and its calibrations?`)) return;
		try {
			await labelStudio.removePrinter(p.id);
			await load();
		} catch (e) {
			fail('Delete', e);
		}
	}

	// ---- calibration ----
	let cal = $state<{
		printer: string;
		sheet: string;
		fr: number;
		fd: number;
		lr: number | null;
		ld: number | null;
	} | null>(null);
	function startCal(p: LabelPrinter, sheet: string) {
		const c = p.calibrations.find((x) => x.sheet === sheet);
		cal = { printer: p.id, sheet, fr: c?.x_mm ?? 0, fd: c?.y_mm ?? 0, lr: null, ld: null };
	}
	async function saveCal(p: LabelPrinter) {
		if (!cal) return;
		try {
			const body =
				cal.lr !== null && cal.ld !== null
					? {
							first: [Number(cal.fr), Number(cal.fd)] as [number, number],
							last: [Number(cal.lr), Number(cal.ld)] as [number, number]
						}
					: { x_mm: Number(cal.fr), y_mm: Number(cal.fd) };
			const next = await labelStudio.calibrate(p.id, cal.sheet, {
				...body,
				tray: p.tray ?? undefined
			});
			printers = printers.map((x) => (x.id === p.id ? next : x));
			notifications.add(`${p.name}: ${cal.sheet} calibrated`, 'success');
			cal = null;
		} catch (e) {
			fail('Calibration', e);
		}
	}
	async function clearCal(p: LabelPrinter, sheet: string) {
		try {
			await labelStudio.clearCalibration(p.id, sheet);
			await load();
		} catch (e) {
			fail('Calibration', e);
		}
	}

	const stateText = (p: LabelPrinter) =>
		p.problem
			? p.problem
			: p.info?.state
				? `${p.info.state}${p.info.state_reasons?.filter((r) => r !== 'none').length ? ` (${p.info.state_reasons.filter((r) => r !== 'none').join(', ')})` : ''}`
				: 'not asked yet';
	const mm = (v: number) => `${v > 0 ? '+' : ''}${v.toFixed(1)}`;
	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
</script>

<div class="flex flex-wrap items-baseline gap-4">
	<p class="max-w-3xl text-sm text-muted-foreground">
		Where labels print. Each printer is asked what it takes: PDF goes as it is; a printer without
		PDF gets the labels as raster (PWG or Apple URF). Calibrate each sheet on each printer: printers
		feed paper their own way.
	</p>
	{#if !adding}
		<button
			onclick={() => (adding = true)}
			class="ml-auto inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
		>
			<Plus class="h-4 w-4" /> Add a printer
		</button>
	{/if}
</div>

{#if adding}
	<form
		class="mt-5 grid gap-4 rounded-md border border-border bg-card p-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]"
		onsubmit={(e) => {
			e.preventDefault();
			add();
		}}
	>
		<div class="space-y-3">
			<h2 class="text-base font-semibold text-foreground">Add a printer</h2>
			<label class={lbl}
				>Address
				<input
					class="{field} font-mono"
					bind:value={form.url}
					placeholder="ipp://192.168.1.143:631/ipp/print"
					required
				/>
				<span class="mt-1 block font-normal"
					>Most printers: <code>ipp://its-address:631/ipp/print</code>. Its own web page or a
					configuration report shows the address.</span
				>
			</label>
			<div class="grid grid-cols-[minmax(0,1fr)_10rem] gap-3">
				<label class={lbl}
					>Name
					<input class={field} bind:value={form.name} placeholder="Office laser" />
				</label>
				<label class={lbl}
					>Id
					<input
						class="{field} font-mono"
						bind:value={form.id}
						oninput={() => (idTouched = true)}
					/>
				</label>
			</div>
			<label class={lbl}
				>Send labels as
				<select class={field} bind:value={form.send}>
					<option value="auto">Whatever it takes best (PDF, else raster)</option>
					<option value="pdf">PDF</option>
					<option value="pwg">PWG Raster</option>
					<option value="urf">Apple URF</option>
				</select>
			</label>
			{#if secure}
				<div class="rounded-md border border-border p-3">
					<p class="text-sm text-foreground">
						An <code>ipps</code> printer is trusted by its certificate.
						<Hint
							inline
							help={{
								line: 'Printers make their own certificates, so no authority vouches for them.',
								more: 'Roastwarden reads the certificate the printer shows now and keeps its SHA-256; afterwards it talks only to a printer showing that same certificate. Compare the fingerprint with the one on the printer’s own web page or configuration report before trusting it.'
							}}
						/>
					</p>
					<button
						type="button"
						class="mt-2 rounded-md border border-border px-3 py-1 text-sm hover:bg-accent"
						onclick={readCertificate}>Read its certificate</button
					>
					{#if cert}
						<p class="mt-2 break-all font-mono text-xs text-foreground">{cert}</p>
						<label class="mt-2 flex items-center gap-2 text-sm text-foreground">
							<input
								type="checkbox"
								checked={form.fingerprint === cert}
								onchange={(e) => (form.fingerprint = e.currentTarget.checked ? cert : '')}
							/>
							This is my printer's certificate: trust it
						</label>
					{/if}
				</div>
			{/if}
			<div class="flex gap-2">
				<button
					type="submit"
					disabled={saving || (secure && !form.fingerprint)}
					class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
					>{saving ? 'Asking it…' : 'Add'}</button
				>
				<button
					type="button"
					class="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent"
					onclick={() => (adding = false)}>Cancel</button
				>
			</div>
		</div>
		<div>
			<button
				type="button"
				onclick={find}
				disabled={finding}
				class="inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent disabled:opacity-50"
			>
				<Search class="h-4 w-4" />
				{finding ? 'Looking for 4 s…' : 'Find printers on the network'}
			</button>
			{#if found}
				{#if found.length}
					<ul class="mt-2 divide-y divide-border rounded-md border border-border">
						{#each found as f (f.url)}
							<li>
								<button
									type="button"
									class="w-full px-3 py-2 text-left text-sm hover:bg-accent"
									onclick={() => pick(f)}
								>
									<span class="font-medium text-foreground">{f.model ?? f.name}</span>
									<span class="block font-mono text-xs text-muted-foreground"
										>{f.url}{f.tls ? ' · ipps too' : ''}{f.color ? ' · colour' : ''}</span
									>
								</button>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="mt-2 text-sm text-muted-foreground">None found. {findNote}</p>
				{/if}
			{/if}
		</div>
	</form>
{/if}

{#if loading}
	<p class="mt-6 text-sm text-muted-foreground">Loading…</p>
{:else if !printers.length && !adding}
	<p class="mt-6 text-sm text-muted-foreground">
		No printer yet. Labels can still be made as PDFs to print yourself.
	</p>
{:else}
	<ul class="mt-5 space-y-4">
		{#each printers as p (p.id)}
			<li class="rounded-md border border-border bg-card p-4">
				<div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
					<h2 class="text-base font-semibold text-foreground">{p.name}</h2>
					{#if p.is_default}<span class="text-xs font-medium text-highlight">default</span>{/if}
					<span class="text-sm text-muted-foreground">{p.info?.make_and_model ?? ''}</span>
					<span class="ml-auto flex items-center gap-3 text-xs">
						<button
							class="inline-flex items-center gap-1 text-action hover:underline"
							onclick={() => check(p)}
							disabled={checking === p.id}
						>
							<RefreshCw class="h-3 w-3 {checking === p.id ? 'animate-spin' : ''}" /> Check now
						</button>
						{#if !p.is_default}<button
								class="text-action hover:underline"
								onclick={() => makeDefault(p)}>Make default</button
							>{/if}
						<button
							class="text-muted-foreground hover:text-destructive"
							onclick={() => remove(p)}
							aria-label="Delete {p.name}"><Trash2 class="h-3.5 w-3.5" /></button
						>
					</span>
				</div>
				<p class="font-mono text-xs text-muted-foreground">
					{p.url}{p.fingerprint ? ` · trusted certificate ${p.fingerprint.slice(0, 16)}…` : ''}
				</p>
				<p class="mt-1 text-sm {p.problem ? 'text-destructive' : 'text-foreground'}">
					{stateText(p)}{#if p.sends}{' · '}labels go as <strong>{p.sends}</strong>{/if}
					{#if asking.includes(p.id)}<span class="text-xs text-muted-foreground">
							· asking it now…</span
						>{/if}
					{#if p.info_at}<span
							class="text-xs text-muted-foreground"
							title={new Date(p.info_at).toLocaleString()}
						>
							· {p.problem ? 'last answered' : 'asked'} {ago(p.info_at)}</span
						>{/if}
				</p>

				<div class="mt-3 grid gap-3 sm:grid-cols-3">
					<label class={lbl}
						>Tray
						<select
							class={field}
							value={p.tray ?? ''}
							onchange={(e) => save(p, { tray: e.currentTarget.value || null })}
						>
							<option value="">The one holding the sheet's size</option>
							{#each p.info?.media_sources ?? [] as t (t)}<option value={t}>{t}</option>{/each}
						</select>
					</label>
					<label class={lbl}
						>Colour
						<select
							class={field}
							value={p.color ?? ''}
							onchange={(e) => save(p, { color: e.currentTarget.value || null })}
						>
							<option value="">Black unless a label has colour</option>
							<option value="monochrome">Always black</option>
							<option value="color">Colour</option>
						</select>
					</label>
					<label class={lbl}
						>Send labels as
						<select
							class={field}
							value={p.send}
							onchange={(e) => save(p, { send: e.currentTarget.value })}
						>
							<option value="auto">Whatever it takes best</option>
							<option value="pdf">PDF</option>
							<option value="pwg">PWG Raster</option>
							<option value="urf">Apple URF</option>
						</select>
					</label>
				</div>

				<h3 class="mt-4 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
					Calibration
				</h3>
				<table class="mt-1 w-full text-sm">
					<tbody>
						{#each sheets as s (s.id)}
							{@const c = p.calibrations.find((x) => x.sheet === s.id)}
							<tr class="border-t border-border">
								<td class="py-1.5 pr-3 text-foreground">{s.name}</td>
								<td class="py-1.5 pr-3 tabular-nums text-muted-foreground">
									{#if c}{mm(c.x_mm)} mm right, {mm(c.y_mm)} mm down{c.tray
											? ` (from ${c.tray})`
											: ''}{:else}the sheet's own offset ({mm(s.offset.x_mm)}, {mm(
											s.offset.y_mm
										)}){/if}
								</td>
								<td class="whitespace-nowrap py-1.5 text-right text-xs">
									<button
										class="text-action hover:underline"
										onclick={() =>
											labelStudio
												.calibrationPdf(s.id, p.id)
												.catch((e) => fail('Calibration page', e))}>Calibration page</button
									>
									·
									<button class="text-action hover:underline" onclick={() => startCal(p, s.id)}
										>Set</button
									>
									{#if c}· <button
											class="text-muted-foreground hover:text-foreground"
											onclick={() => clearCal(p, s.id)}>Clear</button
										>{/if}
								</td>
							</tr>
							{#if cal?.printer === p.id && cal.sheet === s.id}
								<tr
									><td colspan="3" class="pb-3">
										<div class="rounded-md border border-border p-3">
											<p class="text-xs text-muted-foreground">
												Print the calibration page on plain paper, hold it over a sheet of labels
												against the light, and measure how far the marks must move onto the label
												corners: right and down are positive. Measure the first label, and the last
												for a check that the page does not scale.
											</p>
											<div class="mt-2 grid grid-cols-2 gap-3 sm:grid-cols-4">
												<label class={lbl}
													>First label: right, mm<input
														class={field}
														type="number"
														step="0.1"
														bind:value={cal.fr}
													/></label
												>
												<label class={lbl}
													>down, mm<input
														class={field}
														type="number"
														step="0.1"
														bind:value={cal.fd}
													/></label
												>
												<label class={lbl}
													>Last label: right, mm<input
														class={field}
														type="number"
														step="0.1"
														bind:value={cal.lr}
														placeholder="optional"
													/></label
												>
												<label class={lbl}
													>down, mm<input
														class={field}
														type="number"
														step="0.1"
														bind:value={cal.ld}
														placeholder="optional"
													/></label
												>
											</div>
											<div class="mt-2 flex gap-2">
												<button
													class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
													onclick={() => saveCal(p)}>Save</button
												>
												<button
													class="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent"
													onclick={() => (cal = null)}>Cancel</button
												>
											</div>
										</div>
									</td></tr
								>
							{/if}
						{/each}
					</tbody>
				</table>
			</li>
		{/each}
	</ul>
{/if}
