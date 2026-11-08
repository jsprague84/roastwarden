<script lang="ts">
	import type { RoastEvent } from '#lib/api/events.js';
	import type { SessionTelemetryPoint } from '#lib/api/sessions.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';
	import RangeBand from '#lib/components/ui/RangeBand.svelte';
	import type { Judgement } from '#lib/guidance/judge.js';

	interface Props {
		telemetry: SessionTelemetryPoint[];
		events: RoastEvent[];
		/** Roast guidance: FC and drop BT against this profile's own roasts. */
		fcJ?: Judgement | null;
		dropJ?: Judgement | null;
		range?: (r: [number, number]) => string;
		extra?: string[];
	}

	let {
		telemetry,
		events,
		fcJ = null,
		dropJ = null,
		range = () => '',
		extra = []
	}: Props = $props();

	let keyTemps = $derived.by(() => {
		if (!telemetry.length) return { charge: null, turningPoint: null, fc: null, drop: null };

		const chargeEvent = events.find((e) => e.event_type === 'charge');
		const tpEvent = events.find((e) => e.event_type === 'turning_point');
		const charge = chargeEvent?.temperature ?? telemetry[0]?.bean_temp ?? null;
		let turningPoint: number | null = tpEvent?.temperature ?? null;
		if (turningPoint == null) {
			turningPoint = charge;
			for (const t of telemetry) {
				if (t.bean_temp != null && (turningPoint == null || t.bean_temp < turningPoint))
					turningPoint = t.bean_temp;
				else break;
			}
		}
		const fcEvent = events.find((e) => e.event_type === 'first_crack_start');
		const dropEvent = events.find((e) => e.event_type === 'drop');
		return {
			charge,
			turningPoint,
			fc: fcEvent?.temperature ?? null,
			drop: dropEvent?.temperature ?? null
		};
	});

	function fmt(val: number | null | undefined): string {
		return val != null ? `${fmtTemp(val)}${tempUnit()}` : 'N/A';
	}
</script>

<div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
	<div class="rounded-lg border border-border bg-card p-3 text-center">
		<div class="text-xs text-muted-foreground">Charge</div>
		<div class="text-lg font-bold text-foreground">{fmt(keyTemps.charge)}</div>
	</div>
	<div class="rounded-lg border border-border bg-card p-3 text-center">
		<div class="text-xs text-muted-foreground">Turning Point</div>
		<div class="text-lg font-bold text-foreground">{fmt(keyTemps.turningPoint)}</div>
	</div>
	<div class="rounded-lg border border-border bg-card p-3 text-center">
		<div class="text-xs text-muted-foreground">FC Temp</div>
		<div class="text-lg font-bold text-foreground">
			<RangeBand j={keyTemps.fc != null ? fcJ : null} range={fcJ ? range(fcJ.range) : ''} {extra}
				>{fmt(keyTemps.fc)}</RangeBand
			>
		</div>
	</div>
	<div class="rounded-lg border border-border bg-card p-3 text-center">
		<div class="text-xs text-muted-foreground">Drop Temp</div>
		<div class="text-lg font-bold text-foreground">
			<RangeBand
				j={keyTemps.drop != null ? dropJ : null}
				range={dropJ ? range(dropJ.range) : ''}
				{extra}>{fmt(keyTemps.drop)}</RangeBand
			>
		</div>
	</div>
</div>
