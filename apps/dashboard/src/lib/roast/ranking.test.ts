import { describe, it, expect } from 'vitest';
import { filterSessions, sortSessions, toCsv } from './ranking.js';
import type { RoastSession } from '#lib/api/sessions.js';

function s(id: string, over: Partial<RoastSession>): RoastSession {
	return {
		id,
		name: id,
		device_id: 'r',
		profile_id: null,
		status: 'completed',
		start_time: null,
		end_time: null,
		created_at: '2026-10-22T05:00:00Z',
		bean_origin: null,
		bean_variety: null,
		green_weight: null,
		roasted_weight: null,
		notes: null,
		total_time_seconds: null,
		max_temp: null,
		first_crack_time: null,
		development_time_ratio: null,
		weight_loss_pct: null,
		max_ror: null,
		avg_ror_drying: null,
		avg_ror_maillard: null,
		avg_ror_development: null,
		drying_end_time: null,
		drying_end_temp: null,
		auc_value: null,
		...over
	};
}

const list = [
	s('a', {
		roast_purpose: 'espresso',
		development_time_ratio: 0.22,
		bean_origin: 'Ethiopia',
		rating: 4
	}),
	s('b', {
		roast_purpose: 'filter',
		development_time_ratio: 0.17,
		bean_origin: 'Kenya',
		notes: 'juicy, "bright"'
	}),
	s('c', { roast_purpose: 'espresso', status: 'active' }),
	s('d', { roast_purpose: 'espresso', development_time_ratio: 0.2, batch_number: 12 })
];

describe('ranking', () => {
	it('filters by purpose, text and status', () => {
		expect(
			filterSessions(list, { purpose: 'espresso', text: '', completedOnly: true }).map((x) => x.id)
		).toEqual(['a', 'd']);
		expect(
			filterSessions(list, { purpose: '', text: 'ken', completedOnly: false }).map((x) => x.id)
		).toEqual(['b']);
		expect(
			filterSessions(list, { purpose: '', text: 'juicy', completedOnly: false }).map((x) => x.id)
		).toEqual(['b']);
		// A batch number, with or without "#".
		expect(
			filterSessions(list, { purpose: '', text: '#12', completedOnly: false }).map((x) => x.id)
		).toEqual(['d']);
		expect(
			filterSessions(list, { purpose: '', text: '12', completedOnly: false }).map((x) => x.id)
		).toEqual(['d']);
	});
	it('sorts numerically with blanks last', () => {
		expect(sortSessions(list, 'dtr', 'desc').map((x) => x.id)).toEqual(['a', 'd', 'b', 'c']);
		expect(sortSessions(list, 'dtr', 'asc').map((x) => x.id)).toEqual(['b', 'd', 'a', 'c']);
		expect(sortSessions(list, 'rating', 'desc')[0].id).toBe('a');
	});
	it('writes CSV with quoting', () => {
		const csv = toCsv([list[1]]);
		const [head, row] = csv.trim().split('\n');
		expect(head.startsWith('id,#,Date,Name,Bean,For,Time,FC,DTR %')).toBe(true);
		expect(row).toContain('Filter');
		expect(row).toContain('17');
		expect(row).toContain('"juicy, ""bright"""');
	});
});
