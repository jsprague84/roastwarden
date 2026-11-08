<script lang="ts">
	/**
	 * About (docs/notifications.md, "About"): what is running and whether it is well, where
	 * the changes of this revision are written up, and the licence: its text, the source of
	 * this revision and the third-party notices (the image serves them under /licenses/).
	 */
	import { about, downloadArtisanScreen, type AboutInfo } from '#lib/api/about.js';
	import { resolve } from '$app/paths';
	import { ArrowUpRight } from '@lucide/svelte';
	import { busyLines } from '#lib/system/busy.js';
	import { unitState } from '#lib/stores/units.svelte.js';
	import { dashboardIsOlder, versionLine } from '#lib/system/version.js';

	let info = $state<AboutInfo | null>(null);
	let error = $state<string | null>(null);

	async function load() {
		try {
			info = await about.get();
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}
	$effect(() => {
		void load();
		const t = setInterval(load, 30_000);
		return () => clearInterval(t);
	});

	const dashboard = __APP_VERSION__;
	const dashboardRevision = __APP_REVISION__;
	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
	function uptime(s: number) {
		const d = Math.floor(s / 86400);
		const h = Math.floor((s % 86400) / 3600);
		const m = Math.floor((s % 3600) / 60);
		return d ? `${d} d ${h} h` : h ? `${h} h ${m} min` : `${m} min`;
	}
	const short = (rev: string) => (/^[0-9a-f]{7,}$/i.test(rev) ? rev.slice(0, 7) : rev);
	const dt = 'text-muted-foreground';
	const ok = (b: boolean) => (b ? 'text-foreground' : 'text-red-400');
	const link = 'text-foreground underline underline-offset-2';
</script>

{#if error}
	<p class="text-sm text-red-400">Could not reach the server: {error}</p>
{/if}

<div class="divide-y divide-border border-y border-border">
	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Versions</h2>
		<dl class="mt-2 grid grid-cols-[9rem_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
			<dt class={dt}>Server</dt>
			<dd class="text-foreground">
				{info ? versionLine(info.version, info.build) : '…'}
				{#if info?.build?.committed_at}<span class="text-muted-foreground">
						· {when(info.build.committed_at)}</span
					>{/if}
			</dd>
			<dt class={dt}>Revision</dt>
			<dd class="font-mono text-foreground" title={info?.revision}>
				{info ? short(info.revision) : '…'}
			</dd>
			<dt class={dt}>Dashboard</dt>
			<dd class="text-foreground">
				{#if info && (info.version !== dashboard || dashboardIsOlder(dashboardRevision, info.revision))}
					<span class="text-amber-400">Older than the server: reload the page</span>
					<span class="text-muted-foreground"
						>· {dashboard}{dashboardRevision !== 'dev'
							? ` · ${short(dashboardRevision)}`
							: ''}</span
					>
				{:else}
					Same as the server
				{/if}
			</dd>
			<dt class={dt}>Database schema</dt>
			<dd class="min-w-0 break-all text-foreground">{info?.migration ?? '…'}</dd>
		</dl>
		{#if info?.changelog_url}
			<a
				href={info.changelog_url}
				target="_blank"
				rel="external noopener"
				class="mt-3 inline-flex items-center gap-1 text-sm text-foreground underline underline-offset-2"
			>
				What changed (CHANGELOG at this revision) <ArrowUpRight class="h-3.5 w-3.5" />
			</a>
		{/if}
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Health</h2>
		{#if info}
			<dl class="mt-2 grid grid-cols-[9rem_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
				<dt class={dt}>Running for</dt>
				<dd class="text-foreground">
					{uptime(info.uptime_seconds)}
					<span class="text-muted-foreground">· since {when(info.started_at)}</span>
				</dd>
				<dt class={dt}>MQTT broker</dt>
				<dd>
					<span class={ok(info.mqtt_ready)}>{info.mqtt_ready ? 'Connected' : 'Not connected'}</span>
					{#if info.mqtt_broker}
						<span class="text-muted-foreground" data-testid="mqtt-broker"
							>· built in, on {info.mqtt_broker.addr} · {info.mqtt_broker.devices.length
								? `roasters that may sign in: ${info.mqtt_broker.devices.join(', ')}`
								: 'no roaster may sign in yet'} ·
							<a href={resolve('/roasters/passwords')} class="underline hover:text-foreground"
								>MQTT passwords</a
							></span
						>
					{/if}
				</dd>
				<dt class={dt}>Database</dt>
				<dd class={ok(info.db_ready)}>{info.db_ready ? 'Answering' : 'Not answering'}</dd>
				<dt class={dt}>Roasters</dt>
				<dd>
					<span class={info.busy?.busy ? 'text-amber-400' : 'text-foreground'}>
						{info.busy?.busy
							? 'Busy: a deploy waits until this ends'
							: 'Nothing a restart would cut'}
					</span>
					{#each busyLines(info.busy) as line (line)}
						<span class="block text-muted-foreground">{line}</span>
					{/each}
				</dd>
				{#if info.artisan_screen}
					{@const s = info.artisan_screen}
					<dt class={dt}>Artisan screen</dt>
					<dd data-testid="artisan-screen">
						<span class="text-foreground">Modbus TCP port {s.port}</span>
						{#if s.serving}
							<span class="text-muted-foreground"
								>· {s.serving} · {s.writes ? 'Artisan may send commands' : 'read-only'}</span
							>
						{:else}
							<span class="text-amber-400">· serving no roaster: {s.problem}</span>
						{/if}
						<span class="text-muted-foreground"
							>· {s.last_read_at
								? `last read ${when(s.last_read_at)} by ${s.last_client}`
								: 'not read yet'}</span
						>
						<button
							type="button"
							class="ml-1 text-muted-foreground underline underline-offset-2 hover:text-foreground"
							onclick={() => void downloadArtisanScreen(unitState.unit)}
							>Setup for Artisan (.aset)</button
						>
					</dd>
				{/if}
				<dt class={dt}>Last backup</dt>
				<dd>
					{#if info.last_backup}
						{#if info.last_backup.error}
							<span class="text-red-400"
								>Failed {when(info.last_backup.started_at)}: {info.last_backup.error}</span
							>
						{:else}
							<span class="text-foreground">{when(info.last_backup.started_at)}</span>
							<span class="text-muted-foreground"
								>· {info.last_backup.trigger === 'manual' ? 'by hand' : 'nightly'}</span
							>
						{/if}
					{:else}
						<span class="text-muted-foreground">None yet</span>
					{/if}
					<a
						href={resolve('/settings/backups')}
						class="ml-1 text-muted-foreground underline underline-offset-2 hover:text-foreground"
						>Backups</a
					>
				</dd>
				<dt class={dt}>Problems</dt>
				<dd>
					{#if info.problems.length}
						<ul>
							{#each info.problems as p (p.key)}
								<li class="text-amber-400">
									{p.title} <span class="text-muted-foreground">· since {when(p.changed_at)}</span>
								</li>
							{/each}
						</ul>
					{:else}
						<span class="text-foreground">None</span>
					{/if}
				</dd>
			</dl>
		{:else if !error}
			<p class="mt-2 text-sm text-muted-foreground">Loading…</p>
		{/if}
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Support bundle</h2>
		<p class="mt-2 max-w-prose text-sm text-muted-foreground">
			One file for a bug report: the versions, the roasters' setups, the newest roast, settings,
			problems and the recent log. Every password, key and token is removed; green coffee, orders
			and receipts are left out. Roast names and notes are in it: read it before you share it.
		</p>
		<a
			href="/api/support-bundle"
			rel="external"
			download
			class="mt-2 inline-block text-sm {link}"
			data-testid="support-bundle">Download the support bundle</a
		>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Licence</h2>
		<p class="mt-2 max-w-prose text-sm text-muted-foreground">
			Roastwarden is free software under the GNU Affero General Public License, version 3 or any
			later version ({info?.licence ?? 'AGPL-3.0-or-later'}). You may use, change and share it; a
			changed version run for others must offer them its source.
		</p>
		<ul class="mt-2 space-y-1 text-sm">
			<li>
				<a href="/licenses/LICENSE.txt" target="_blank" rel="external noopener" class={link}
					>Licence text</a
				>
			</li>
			{#if info?.source_url}
				<li>
					<a
						href={info.source_url}
						target="_blank"
						rel="external noopener"
						class="inline-flex items-center gap-1 {link}"
					>
						Source code of this revision <ArrowUpRight class="h-3.5 w-3.5" />
					</a>
				</li>
			{/if}
			<li>
				<span class="text-muted-foreground">Third-party notices:</span>
				<a href="/licenses/server.txt" target="_blank" rel="external noopener" class={link}
					>server</a
				>,
				<a href="/licenses/dashboard.txt" target="_blank" rel="external noopener" class={link}
					>dashboard</a
				>
			</li>
		</ul>
	</section>
</div>
