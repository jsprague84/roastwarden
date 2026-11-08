<script lang="ts">
	/**
	 * Live AUC against the profile (docs/statistics.md): the roast's area under BT so far,
	 * the profile's at the same roast time, and the profile's at its end as the target.
	 */
	import { settings as settingsApi } from '#lib/api/settings.js';
	import { roast } from '#lib/stores/roast.svelte.js';
	import { profileState } from '#lib/stores/profile.svelte.js';
	import { telemetryHistory } from '#lib/stores/telemetry.js';
	import {
		aucConfig,
		liveAuc,
		DEFAULT_AUC,
		type AucConfig,
		type AucStart
	} from '#lib/roast/stats.js';
	import { fmtDelta, tempUnit } from '#lib/stores/units.svelte.js';

	let cfg = $state<AucConfig>(DEFAULT_AUC);
	$effect(() => {
		settingsApi
			.get()
			.then((v) => (cfg = aucConfig(v)))
			.catch(() => {});
	});

	const at = (type: string) =>
		roast.events.find((e) => e.event_type === type)?.elapsed_seconds ?? null;
	const startMs = $derived(roast.session?.start_time ? Date.parse(roast.session.start_time) : null);
	const chargeAt = $derived(at('charge'));
	const dropAt = $derived(at('drop'));

	/** Where the AUC starts, roast time; null until that mark exists (then nothing counts yet). */
	function roastStart(start: AucStart, charge: number): number | null {
		if (start === 'charge') return 0;
		const t = at(start);
		return t == null ? null : t - charge;
	}
	function profileStart(start: AucStart): number | null {
		const p = profileState.activeProfile;
		if (!p || start === 'charge') return 0;
		if (start === 'first_crack_start' && p.target_first_crack) return p.target_first_crack;
		return p.events?.find((e) => e.action === 'landmark' && e.label === start)?.at_seconds ?? null;
	}

	const value = $derived.by(() => {
		if (startMs == null || chargeAt == null) return null;
		const end = dropAt != null ? dropAt - chargeAt : Infinity;
		const pts: [number, number][] = [];
		for (const h of $telemetryHistory) {
			const t = (h.timestamp - startMs) / 1000 - chargeAt;
			if (t < 0 || t > end || h.telemetry.beanTemp == null) continue;
			pts.push([t, h.telemetry.beanTemp]);
		}
		const from = roastStart(cfg.start, chargeAt);
		if (from == null) return { waiting: true as const };
		const p = profileState.activeProfile;
		const prof: [number, number][] | null = p?.points?.length
			? p.points
					.filter((x) => x.target_temp != null)
					.map((x) => [x.time_seconds, x.target_temp] as [number, number])
			: null;
		const pFrom = profileStart(cfg.start);
		const v = liveAuc(pts, pFrom == null ? null : prof, cfg, from, pFrom ?? 0);
		return v ? { waiting: false as const, ...v } : null;
	});
	const unit = $derived(`${tempUnit()}·min`);
</script>

{#if value}
	<p
		class="text-xs text-muted-foreground"
		title="Area under BT{cfg.base ? ' above the base temperature' : ''} from {cfg.start === 'charge'
			? 'CHARGE'
			: cfg.start.replace(
					/_/g,
					' '
				)} (Settings → Roasting). The profile's at the same roast time, and at its end."
	>
		{#if value.waiting}
			AUC starts at {cfg.start.replace(/_/g, ' ')}
		{:else}
			<span class="font-medium text-foreground">AUC {fmtDelta(value.now, 0)}</span>
			{unit}
			{#if value.profileNow != null}
				· profile {fmtDelta(value.profileNow, 0)} now
				<span
					class={Math.abs(value.now - value.profileNow) > Math.max(30, value.profileNow * 0.05)
						? 'text-amber-400'
						: ''}>({fmtDelta(value.now - value.profileNow, 0, true)})</span
				>
			{/if}
			{#if value.target != null}· target {fmtDelta(value.target, 0)} at drop{/if}
		{/if}
	</p>
{/if}
