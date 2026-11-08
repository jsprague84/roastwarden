import { describe, it, expect } from 'vitest';
import { roastSummary, phaseRows, rorSeries, btAt, fmtMmSs } from './report.js';
import type { RoastEvent } from '#lib/api/events.js';
import type { SessionTelemetryPoint } from '#lib/api/sessions.js';

const ev = (
	event_type: string,
	elapsed_seconds: number,
	temperature: number | null = null
): RoastEvent => ({
	id: event_type,
	session_id: 's',
	event_type,
	elapsed_seconds,
	temperature,
	notes: null,
	created_at: ''
});

// Session starts 10 s before CHARGE; BT climbs 0.25 °C/s from 30 °C at CHARGE; samples through cooling.
const telemetry: SessionTelemetryPoint[] = Array.from({ length: 761 }, (_, i) => ({
	id: String(i),
	session_id: 's',
	timestamp: '',
	elapsed_seconds: i,
	bean_temp: i < 10 ? 30 : i <= 700 ? 30 + (i - 10) * 0.25 : 202.5 - (i - 700),
	env_temp: null,
	rate_of_rise: null,
	heater: null,
	fan: null,
	setpoint: null
}));
const events = [
	ev('charge', 10),
	ev('drying_end', 290, 150),
	ev('first_crack_start', 530, 160),
	ev('drop', 670, 195)
];

describe('roast report', () => {
	it('measures from CHARGE to DROP', () => {
		const s = roastSummary(events, telemetry);
		expect(s.chargeAt).toBe(10);
		expect(s.total).toBe(660);
		expect(s.fc?.t).toBe(520);
		expect(s.dtr).toBeCloseTo((140 / 660) * 100, 6);
	});
	it('builds the phase table', () => {
		const rows = phaseRows(roastSummary(events, telemetry), telemetry);
		expect(rows.map((r) => r.key)).toEqual(['drying', 'maillard', 'development']);
		expect(rows[0].duration).toBe(280);
		expect(rows[2].duration).toBe(140);
		expect(rows[1].dT).toBeCloseTo(60, 6);
		expect(rows[1].ror).toBeCloseTo(15, 6);
		// Drying from TP when there is one.
		const withTp = phaseRows(
			roastSummary([...events, ev('turning_point', 70, 45)], telemetry),
			telemetry
		);
		expect(withTp[0].dT).toBeCloseTo(100 - 45, 6);
		expect(withTp[0].ror).toBeCloseTo(15, 6);
		expect(rows.reduce((a, r) => a + (r.pct ?? 0), 0)).toBeCloseTo(100, 6);
	});
	it('computes a smoothed RoR and interpolates BT', () => {
		const r = rorSeries(telemetry, 10);
		const mid = r.find(([t]) => t === 300)!;
		expect(mid[1]).toBeCloseTo(15, 6);
		expect(btAt(telemetry, 10, 100)).toBeCloseTo(55, 6);
		expect(fmtMmSs(545)).toBe('9:05');
		expect(fmtMmSs(null)).toBe('–');
	});
});
