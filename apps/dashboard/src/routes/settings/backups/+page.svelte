<script lang="ts">
	/**
	 * Backups (docs/DEPLOY.md "Backups"): the nightly copy, "Back up now", retention and the
	 * hour, and a checked restore that goes in at the next start of the server.
	 */
	import { backups, type BackupInspection, type BackupOverview } from '#lib/api/backups.js';
	import { settings } from '#lib/api/settings.js';
	import { notifications } from '#lib/stores/notifications.js';

	let view = $state<BackupOverview | null>(null);
	let busy = $state(false);
	let keep = $state<number | string>(14);
	let hour = $state<number | string>(3);
	let saved = $state(false);
	let checks = $state<Record<string, BackupInspection | string>>({});
	let confirmRestore = $state<string | null>(null);

	async function load() {
		try {
			view = await backups.overview();
			keep = view.keep;
			hour = view.hour_utc;
		} catch (e) {
			notifications.add(
				`Could not read the backups: ${e instanceof Error ? e.message : e}`,
				'error'
			);
		}
	}
	$effect(() => {
		void load();
	});

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	async function backUpNow() {
		busy = true;
		try {
			const r = await backups.now();
			notifications.add(
				`Backup written: ${r.file.name}${r.removed ? `, ${r.removed} old one${r.removed === 1 ? '' : 's'} removed` : ''}`,
				'success'
			);
		} catch (e) {
			notifications.add(`Backup failed: ${msg(e)}`, 'error');
		} finally {
			busy = false;
			await load();
		}
	}

	async function saveSchedule() {
		const k = Math.max(0, Math.min(365, Math.round(Number(keep))));
		const h = Math.max(0, Math.min(23, Math.round(Number(hour))));
		if (!Number.isFinite(k) || !Number.isFinite(h)) return;
		await Promise.all([
			settings.set('backup_keep', String(k)),
			settings.set('backup_hour', String(h))
		]);
		saved = true;
		setTimeout(() => (saved = false), 2000);
		await load();
	}

	async function check(name: string) {
		try {
			checks = { ...checks, [name]: await backups.check(name) };
		} catch (e) {
			checks = { ...checks, [name]: msg(e) };
		}
	}

	async function restore(name: string) {
		busy = true;
		try {
			const r = await backups.restore(name);
			checks = { ...checks, [name]: r.inspection };
			notifications.add('Restore staged: it goes in when the server restarts', 'success');
		} catch (e) {
			checks = { ...checks, [name]: msg(e) };
			notifications.add(`Restore refused: ${msg(e)}`, 'error');
		} finally {
			confirmRestore = null;
			busy = false;
			await load();
		}
	}

	async function cancelRestore() {
		try {
			await backups.cancelRestore();
			notifications.add('Staged restore cancelled', 'success');
		} catch (e) {
			notifications.add(`Could not cancel: ${msg(e)}`, 'error');
		}
		await load();
	}

	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
	const size = (b: number | null | undefined) =>
		b == null ? '' : b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.round(b / 1e3)} kB`;
	/** The UTC hour in this browser's time. */
	const localHour = (h: number) => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- a local scratch value for one format
		const d = new Date();
		d.setUTCHours(h, 0, 0, 0);
		return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	};
	const next = $derived.by(() => {
		if (!view?.next_run) return 'off (keep is 0)';
		const t = new Date(view.next_run);
		return t.getTime() <= Date.now()
			? 'due now: within a few minutes, once no roast is running'
			: when(view.next_run);
	});
	const describe = (i: BackupInspection) =>
		`Restorable: ${i.roasts} roast${i.roasts === 1 ? '' : 's'}, ${i.profiles} profiles, ${i.beans} bean${i.beans === 1 ? '' : 's'}; schema ${i.schema}.`;

	const input =
		'w-20 rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const primary =
		'rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50';
	const secondary =
		'rounded-md border border-border px-2.5 py-1 text-xs font-medium text-foreground hover:bg-accent disabled:opacity-50';
</script>

<p class="text-sm text-muted-foreground">
	The server copies its database every night: every roast, profile, bean and setting, in one file.
	Copies stay on the server's data volume.
</p>

{#if view?.restore_staged}
	<div
		class="mt-4 flex flex-wrap items-center gap-3 border-l-2 border-amber-500 pl-3 text-sm"
		role="status"
	>
		<span class="flex-1 text-foreground"
			>A restore is staged. It replaces the database when the server next starts (<code
				class="text-xs">docker restart roastwarden</code
			>); the current one is moved aside, not deleted.</span
		>
		<button class={secondary} onclick={cancelRestore}>Cancel restore</button>
	</div>
{/if}

<div class="mt-4 divide-y divide-border border-y border-border">
	<section class="py-5">
		<div class="flex flex-wrap items-start justify-between gap-3">
			<div class="text-sm">
				<h2 class="font-semibold text-foreground">Last backup</h2>
				{#if view?.last}
					{#if view.last.error}
						<p class="mt-1 text-red-400">Failed {when(view.last.started_at)}: {view.last.error}</p>
					{:else}
						<p class="mt-1 text-foreground">
							{when(view.last.started_at)}
							<span class="text-muted-foreground"
								>· {view.last.trigger === 'manual' ? 'by hand' : 'nightly'} · {size(
									view.last.size_bytes
								)}</span
							>
						</p>
					{/if}
				{:else}
					<p class="mt-1 text-muted-foreground">None yet.</p>
				{/if}
				<p class="mt-1 text-muted-foreground">Next nightly: {next}</p>
			</div>
			<button class={primary} onclick={backUpNow} disabled={busy || view?.running}
				>{busy || view?.running ? 'Backing up…' : 'Back up now'}</button
			>
		</div>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Schedule</h2>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			Keep the newest copies (0 turns the nightly copy off; Back up now still works). The hour is
			UTC; a roast in progress delays it, and a server that was down catches up when it starts.
		</p>
		<div class="mt-2 flex flex-wrap items-end gap-3">
			<label class="text-sm text-foreground"
				>Keep
				<input
					type="number"
					min="0"
					max="365"
					step="1"
					bind:value={keep}
					class="{input} mt-1 block"
				/>
			</label>
			<label class="text-sm text-foreground"
				>Hour (UTC)
				<input
					type="number"
					min="0"
					max="23"
					step="1"
					bind:value={hour}
					class="{input} mt-1 block"
				/>
			</label>
			<span class="pb-2 text-sm text-muted-foreground">= {localHour(Number(hour) || 0)} here</span>
			<button class={primary} onclick={saveSchedule}>Save</button>
			{#if saved}<span class="pb-2 text-sm text-green-400">Saved.</span>{/if}
		</div>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Copies</h2>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			Check proves a copy is whole and fits this version. Restore checks it the same way, then puts
			it in place at the next start of the server.
		</p>
		{#if view && view.files.length}
			<ul class="mt-3 divide-y divide-border border-y border-border text-sm">
				{#each view.files as f (f.name)}
					{@const c = checks[f.name]}
					<li class="py-2">
						<div class="flex flex-wrap items-center gap-2">
							<span class="text-foreground">{when(f.created_at)}</span>
							<span class="text-xs text-muted-foreground">{size(f.bytes)}</span>
							<span class="ml-auto flex gap-2">
								<button class={secondary} onclick={() => check(f.name)}>Check</button>
								<button class={secondary} onclick={() => (confirmRestore = f.name)} disabled={busy}
									>Restore…</button
								>
							</span>
						</div>
						{#if c}
							<p
								class="mt-1 text-xs {typeof c === 'string'
									? 'text-red-400'
									: 'text-muted-foreground'}"
							>
								{typeof c === 'string' ? c : describe(c)}
							</p>
						{/if}
						{#if confirmRestore === f.name}
							<div
								class="mt-2 flex flex-wrap items-center gap-2 border-l-2 border-red-500 pl-2 text-xs"
								role="alert"
							>
								<span class="flex-1 text-foreground"
									>Replace the database with this copy at the next start? Roasts since {when(
										f.created_at
									)} are not in it.</span
								>
								<button
									class="rounded-md bg-red-600 px-2.5 py-1 font-medium text-white hover:bg-red-700 disabled:opacity-50"
									onclick={() => restore(f.name)}
									disabled={busy}>Stage restore</button
								>
								<button class={secondary} onclick={() => (confirmRestore = null)}
									>Keep current</button
								>
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{:else if view}
			<p class="mt-2 text-sm text-muted-foreground">No copies yet.</p>
		{/if}
		{#if view}<p class="mt-2 text-xs text-muted-foreground">
				Folder on the server: <code>{view.dir}</code>
			</p>{/if}
	</section>
</div>
