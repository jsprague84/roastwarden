<script lang="ts">
	/** The profile library: what each profile targets, import from Artisan or a file, and
	 *  from another server's share link (docs/profiles.md, "Sharing a profile"). */
	import { resolve } from '$app/paths';
	import { goto } from '$app/navigation';
	import { profileShare, type LinkImport } from '#lib/api/profileShare.js';
	import { profiles, type RoastProfile } from '#lib/api/profiles.js';
	import { levelLabel } from '#lib/guidance/references.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { roast } from '#lib/stores/roast.svelte.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import { fmtClock, profileDtr } from '#lib/scope/scopeData.js';
	import { kindTag, selectedKind, sortByKind } from '#lib/profile/kind.js';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	// The selected roaster's kind first; profiles for another kind last, marked (docs/profiles.md).
	const kind = $derived(selectedKind(roast.setup, roast.deviceId));
	const list = $derived(sortByKind(data.profiles as RoastProfile[], kind));

	let fileInput = $state<HTMLInputElement | null>(null);
	let importing = $state(false);

	async function importFile(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		importing = true;
		try {
			const text = await file.text();
			// The file's own title names it (Artisan's title, or the exported profile's name).
			const imported = await profiles.importArtisan(text);
			notifications.add(`Imported ${imported.name}`, 'success');
			goto(resolve('/profiles/[id]', { id: imported.id }));
		} catch (err) {
			notifications.add(err instanceof Error ? err.message : 'Import failed', 'error');
		} finally {
			importing = false;
		}
	}

	// Import from a share link: read it first (nothing saved), then import
	let linkOpen = $state(false);
	let link = $state('');
	let linkBusy = $state(false);
	let linkRead = $state<LinkImport | null>(null);

	async function readLink() {
		linkBusy = true;
		linkRead = null;
		try {
			linkRead = await profileShare.importLink(link.trim(), { preview: true });
		} catch (err) {
			notifications.add(err instanceof Error ? err.message : 'Could not read the link', 'error');
		} finally {
			linkBusy = false;
		}
	}
	async function importLink() {
		linkBusy = true;
		try {
			const r = await profileShare.importLink(link.trim());
			if (r.saved) {
				notifications.add(`Imported ${r.saved.name}`, 'success');
				goto(resolve('/profiles/[id]', { id: r.saved.id }));
			}
		} catch (err) {
			notifications.add(err instanceof Error ? err.message : 'Import failed', 'error');
		} finally {
			linkBusy = false;
		}
	}
	function linkSummary(r: LinkImport): string {
		const p = r.profile;
		const parts = [
			p.roast_level
				? `${levelLabel(p.roast_level)}${p.roast_purpose ? ` for ${p.roast_purpose}` : ''}`
				: p.roast_purpose,
			p.target_first_crack ? `first crack ${fmtClock(p.target_first_crack)}` : null,
			p.target_total_time ? `drop ${fmtClock(p.target_total_time)}` : null,
			`${p.points.length} points, ${p.events.length} events`,
			p.shared_by ? `shared by ${p.shared_by}` : null
		];
		return parts.filter(Boolean).join(' · ');
	}

	function made(p: RoastProfile): string {
		return [p.beans, p.batch_size_g ? fmtGrams(p.batch_size_g) : null, p.roaster]
			.filter(Boolean)
			.join(' · ');
	}
	function dropText(p: RoastProfile): string {
		if (!p.target_total_time) return '—';
		return `${fmtClock(p.target_total_time)}${p.target_end_temp ? ` @ ${fmtTemp(p.target_end_temp, 0)}${tempUnit()}` : ''}`;
	}
	function dtrText(p: RoastProfile): string {
		const d = profileDtr(p.target_first_crack, p.target_total_time);
		return d != null ? `${d.toFixed(0)} %` : '—';
	}

	const th = 'px-3 py-2 text-left text-xs font-medium text-muted-foreground';
</script>

<svelte:head>
	<title>Profiles | Roastwarden</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<h1 class="text-2xl font-bold text-foreground">Profiles</h1>
			<p class="mt-1 text-sm text-muted-foreground">
				Target curves for a roast. Save a good roast as one from its page in History.
			</p>
		</div>
		<div class="flex gap-2">
			<input
				bind:this={fileInput}
				type="file"
				accept=".alog,.json"
				class="hidden"
				onchange={importFile}
			/>
			<button
				onclick={() => fileInput?.click()}
				disabled={importing}
				class="rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
				title="An Artisan roast (.alog) or a Roastwarden profile (.json)"
			>
				{importing ? 'Importing…' : 'Import .alog / .json'}
			</button>
			<button
				onclick={() => (linkOpen = !linkOpen)}
				aria-expanded={linkOpen}
				class="rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent"
				title="Another Roastwarden's shared profile (https://…/P/…)">Import from a link</button
			>
			<a
				href={resolve('/profiles/new')}
				class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
				>New profile</a
			>
		</div>
	</div>

	{#if linkOpen}
		<section
			class="max-w-3xl space-y-2 rounded-md border border-border bg-card px-3 py-3 text-sm"
			data-testid="import-link"
		>
			<p class="text-muted-foreground">
				Paste a shared profile's page (<code>https://…/P/…</code>). The server reads it, shows what
				it is, and imports it when you say so.
			</p>
			<form
				class="flex flex-wrap gap-2"
				onsubmit={(e) => {
					e.preventDefault();
					readLink();
				}}
			>
				<input
					bind:value={link}
					oninput={() => (linkRead = null)}
					placeholder="https://roastwarden.example/P/…"
					class="min-w-0 flex-1 rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
				/>
				<button
					type="submit"
					disabled={linkBusy || !link.trim()}
					class="rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50"
					>{linkBusy && !linkRead ? 'Reading…' : 'Read'}</button
				>
			</form>
			{#if linkRead}
				<div class="flex flex-wrap items-center justify-between gap-2 border-t border-border pt-2">
					<div class="min-w-0">
						<div class="font-medium text-foreground">{linkRead.profile.name}</div>
						<div class="text-xs text-muted-foreground">{linkSummary(linkRead)}</div>
					</div>
					<button
						onclick={importLink}
						disabled={linkBusy}
						class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
						>Import it</button
					>
				</div>
			{/if}
		</section>
	{/if}

	{#if list.length === 0}
		<p class="py-8 text-center text-sm text-muted-foreground">
			No profiles yet. Design one, import one from Artisan, or save a roast as a profile.
		</p>
	{:else}
		<!-- Phone: one card per profile -->
		<ul class="divide-y divide-border rounded-md border border-border bg-card sm:hidden">
			{#each list as p (p.id)}
				<li>
					<a href={resolve('/profiles/[id]', { id: p.id })} class="block px-3 py-2.5">
						<div class="flex items-baseline justify-between gap-2">
							<span class="font-medium text-foreground">{p.name}</span>
							{#if roast.previewProfileId === p.id}<span class="text-[11px] text-amber-400"
									>next roast</span
								>
							{:else if kindTag(p.roaster_kind, kind)}<span
									class="text-[11px] text-muted-foreground">{kindTag(p.roaster_kind, kind)}</span
								>{/if}
						</div>
						{#if made(p)}<div class="text-xs text-muted-foreground">{made(p)}</div>{/if}
						<div class="mt-0.5 text-xs tabular-nums text-muted-foreground">
							FC {p.target_first_crack ? fmtClock(p.target_first_crack) : '—'} · drop {dropText(p)} ·
							DTR {dtrText(p)}
						</div>
					</a>
				</li>
			{/each}
		</ul>

		<!-- Wider: a table -->
		<div class="hidden overflow-x-auto rounded-md border border-border bg-card sm:block">
			<table class="w-full text-sm">
				<thead class="border-b border-border">
					<tr>
						<th class={th}>Profile</th>
						<th class={th}>Made for</th>
						<th class="{th} text-right">First crack</th>
						<th class="{th} text-right">Drop</th>
						<th class="{th} text-right">DTR</th>
						<th class="{th} text-right">Charge</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-border">
					{#each list as p (p.id)}
						<tr
							class="cursor-pointer hover:bg-accent"
							onclick={() => goto(resolve('/profiles/[id]', { id: p.id }))}
						>
							<td class="px-3 py-2">
								<a
									href={resolve('/profiles/[id]', { id: p.id })}
									class="font-medium text-foreground hover:underline">{p.name}</a
								>
								{#if roast.previewProfileId === p.id}<span class="ml-2 text-[11px] text-amber-400"
										>next roast</span
									>{/if}
								{#if kindTag(p.roaster_kind, kind)}<span
										class="ml-2 text-[11px] text-muted-foreground"
										>{kindTag(p.roaster_kind, kind)}</span
									>{/if}
								{#if p.description}<div class="line-clamp-1 max-w-md text-xs text-muted-foreground">
										{p.description}
									</div>{/if}
							</td>
							<td class="px-3 py-2 text-xs text-muted-foreground">{made(p) || '—'}</td>
							<td class="px-3 py-2 text-right tabular-nums"
								>{p.target_first_crack ? fmtClock(p.target_first_crack) : '—'}</td
							>
							<td class="px-3 py-2 text-right tabular-nums">{dropText(p)}</td>
							<td class="px-3 py-2 text-right tabular-nums">{dtrText(p)}</td>
							<td class="px-3 py-2 text-right tabular-nums"
								>{p.charge_temp != null ? `${fmtTemp(p.charge_temp, 0)}${tempUnit()}` : '—'}</td
							>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
