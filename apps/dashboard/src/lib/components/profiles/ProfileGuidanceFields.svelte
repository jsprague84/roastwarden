<script lang="ts">
	/**
	 * What a profile is for (docs/roast-guidance.md): roast level, purpose, and the expected
	 * ranges, which default to the level's cited references. Empty fields use the defaults.
	 */
	import Hint from '#lib/components/ui/Hint.svelte';
	import LevelStrip from '#lib/components/profiles/LevelStrip.svelte';
	import type { ProfileExpectations } from '#lib/api/profiles.js';
	import { PURPOSES } from '#lib/roast/properties.js';
	import {
		bandText,
		defaultsFor,
		isLevel,
		sourceById,
		LEVELS,
		METRICS,
		PURPOSE_LEVEL_NOTE,
		type MetricKey
	} from '#lib/guidance/references.js';

	let {
		level = $bindable(''),
		purpose = $bindable(''),
		expectations = $bindable(null)
	}: { level?: string; purpose?: string; expectations?: ProfileExpectations | null } = $props();

	const defaults = $derived(defaultsFor(isLevel(level) ? level : null));
	const UNIT: Record<MetricKey, string> = {
		weight_loss_pct: '%',
		development_s: 's',
		dtr_pct: '%',
		color_ground: 'Agtron, ground'
	};

	// Text fields per metric, written back as [low, high] when both are numbers.
	let text = $state<Record<MetricKey, [string, string]>>(
		Object.fromEntries(
			METRICS.map((m) => [
				m.key,
				(expectations?.[m.key] ?? ['', '']).map((v) => String(v)) as [string, string]
			])
		) as Record<MetricKey, [string, string]>
	);
	const overrides = $derived(
		METRICS.filter((m) => text[m.key][0] !== '' || text[m.key][1] !== '').length
	);

	function write() {
		const out: ProfileExpectations = {};
		for (const m of METRICS) {
			const lo = parseFloat(text[m.key][0]);
			const hi = parseFloat(text[m.key][1]);
			if (Number.isFinite(lo) && Number.isFinite(hi))
				out[m.key] = [Math.min(lo, hi), Math.max(lo, hi)];
		}
		expectations = Object.keys(out).length ? out : null;
	}

	function help(key: MetricKey) {
		const d = defaults[key];
		if (!d)
			return { help: { line: 'No default for this level; set a range to judge it.' }, links: [] };
		return {
			help: {
				line: `${LEVELS.find((l) => l.key === level)?.label} default: ${d.range[0]}–${d.range[1]} ${UNIT[key]}.`,
				more: `${d.basis}${d.derived ? ' A Roastwarden default from the cited points, not a quoted standard.' : ''}`
			},
			links: d.sources
				.map((id) => sourceById(id))
				.filter((s) => s != null)
				.map((s) => ({ title: s.title, url: s.url }))
		};
	}
</script>

<div>
	<label
		for="profile-level"
		class="mb-1 flex items-center gap-1.5 text-xs font-medium text-muted-foreground"
	>
		Roast level
		<Hint
			inline
			help={{
				line: 'The SCA colour tiles on a ground Agtron reading. Roast guidance reads weight loss, colour and development against it.',
				more: PURPOSE_LEVEL_NOTE
			}}
			links={[{ title: sourceById('agtron2018')!.title, url: sourceById('agtron2018')!.url }]}
		/>
	</label>
	<select
		id="profile-level"
		bind:value={level}
		class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
	>
		<option value="">Not set</option>
		{#each LEVELS as l (l.key)}<option value={l.key}>{l.label} (Agtron {bandText(l.agtron)})</option
			>{/each}
	</select>
	<LevelStrip level={level || null} class="mt-1.5" />
</div>
<div>
	<label for="profile-purpose" class="mb-1 block text-xs font-medium text-muted-foreground"
		>Purpose</label
	>
	<select
		id="profile-purpose"
		bind:value={purpose}
		class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
	>
		<option value="">Not set</option>
		{#each PURPOSES as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
	</select>
</div>
<details class="sm:col-span-2" open={overrides > 0 ? true : undefined}>
	<summary class="cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground">
		Expected{overrides ? ` · ${overrides} set on this profile` : ': the level’s defaults'}
	</summary>
	<div class="mt-2 grid gap-x-4 gap-y-2 sm:grid-cols-2">
		{#each METRICS as m (m.key)}
			{@const h = help(m.key)}
			<div>
				<div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
					{m.label} ({UNIT[m.key]}) <Hint inline help={h.help} links={h.links} />
				</div>
				<div class="flex items-center gap-1.5">
					<input
						type="text"
						inputmode="decimal"
						aria-label="{m.label} low"
						bind:value={text[m.key][0]}
						oninput={write}
						placeholder={defaults[m.key] ? String(defaults[m.key]!.range[0]) : ''}
						class="w-full rounded border border-border bg-input px-2 py-1 text-sm text-foreground placeholder:text-muted-foreground/70"
					/>
					<span class="text-muted-foreground">–</span>
					<input
						type="text"
						inputmode="decimal"
						aria-label="{m.label} high"
						bind:value={text[m.key][1]}
						oninput={write}
						placeholder={defaults[m.key] ? String(defaults[m.key]!.range[1]) : ''}
						class="w-full rounded border border-border bg-input px-2 py-1 text-sm text-foreground placeholder:text-muted-foreground/70"
					/>
				</div>
			</div>
		{/each}
	</div>
	<p class="mt-1.5 text-xs text-muted-foreground">
		Empty fields use the level’s defaults. Temperatures and times are judged against this profile’s
		own roasts, never a published number.
	</p>
</details>
