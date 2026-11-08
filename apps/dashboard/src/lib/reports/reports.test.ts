import { describe, expect, it } from 'vitest';
import {
	compareFileName,
	comparePair,
	compareParams,
	cuppingFileName,
	cuppingParams,
	datedFileName,
	newestOf,
	reportError,
	spanParams,
	reportFileName,
	reportPath,
	reportQuery,
	reportableRoasts,
	roastLabel,
	traceParams,
	tzMinutes
} from './reports.js';

describe('reports', () => {
	it('asks only for what was chosen', () => {
		expect(reportQuery('svg', { paper: '', units: '' }, -300)).toBe('format=svg&tz=-300');
		expect(reportQuery('pdf', { paper: 'a4', units: 'F' }, 60)).toBe(
			'format=pdf&tz=60&paper=a4&units=F'
		);
	});

	it('knows where a report is and what its file is called', () => {
		expect(reportPath('roast', 'a b')).toBe('/api/reports/roast/a%20b');
		expect(reportFileName('roast', { id: 'abcdef123456', batch_number: 5 })).toBe(
			'roast-report-batch-5.pdf'
		);
		expect(reportFileName('roast', { id: 'abcdef123456' })).toBe('roast-report-abcdef12.pdf');
	});

	it('offers finished roasts, newest first, by batch and name', () => {
		const list = [
			{
				id: 'a',
				name: 'Old',
				batch_number: 1,
				start_time: '2026-10-20T10:00:00Z',
				created_at: '2026-10-20T10:00:00Z',
				status: 'completed'
			},
			{
				id: 'b',
				name: 'Running',
				batch_number: null,
				start_time: '2026-10-26T10:00:00Z',
				created_at: '2026-10-26T10:00:00Z',
				status: 'active'
			},
			{
				id: 'c',
				name: 'New',
				batch_number: 2,
				start_time: '2026-10-25T10:00:00Z',
				created_at: '2026-10-25T10:00:00Z',
				status: 'completed'
			}
		];
		expect(reportableRoasts(list).map((s) => s.id)).toEqual(['c', 'a']);
		expect(roastLabel(list[2], 'en-GB')).toBe('#2 · New · 25 Oct 2026');
	});

	it('gives the time zone east of UTC', () => {
		const d = new Date('2026-10-26T12:00:00Z');
		expect(tzMinutes(d)).toBe(-d.getTimezoneOffset());
	});

	it('asks for a cupping sheet: a blank form or the scores', () => {
		const base = {
			mode: 'blank' as const,
			roasts: ['a', 'b'],
			samples: 1,
			perPage: 2 as const,
			date: ''
		};
		expect(cuppingParams(base)).toBe('roasts=a%2Cb&blank=true&samples=1');
		expect(cuppingParams({ ...base, perPage: 6, samples: 0 })).toBe(
			'roasts=a%2Cb&blank=true&per_page=6'
		);
		expect(cuppingParams({ ...base, mode: 'scores' })).toBe('roasts=a%2Cb');
		expect(cuppingParams({ ...base, mode: 'scores', roasts: [], date: '2026-10-26' })).toBe(
			'date=2026-10-26'
		);
		expect(cuppingParams({ ...base, mode: 'scores', roasts: [] })).toBe('');
		expect(cuppingFileName('blank', new Date(2026, 9, 26))).toBe('cupping-sheet-2026-10-26.pdf');
		expect(datedFileName('stock-take', new Date(2026, 0, 5))).toBe('stock-take-2026-01-05.pdf');
	});

	it('asks for a period by month, year or range', () => {
		const jan = new Date(2026, 0, 15);
		const oct = new Date(2026, 9, 26);
		expect(spanParams('this-month', oct)).toBe('month=2026-10');
		expect(spanParams('last-month', oct)).toBe('month=2026-09');
		expect(spanParams('last-month', jan)).toBe('month=2025-12');
		expect(spanParams('this-year', oct)).toBe('year=2026');
		expect(spanParams('last-year', oct)).toBe('year=2025');
		expect(spanParams('range', oct, '2026-10-01', '2026-10-10')).toBe(
			'from=2026-10-01&to=2026-10-10'
		);
	});

	it("starts a coffee's or profile's report on the newest roast's", () => {
		const roasts = [
			{ bean_id: null, profile_id: 'gone' },
			{ bean_id: 'guji', profile_id: 'light' },
			{ bean_id: 'aaa' }
		];
		const coffees = [{ id: 'aaa' }, { id: 'guji' }];
		expect(newestOf(roasts, 'bean_id', coffees)).toBe('guji');
		expect(newestOf(roasts, 'profile_id', [{ id: 'dark' }, { id: 'light' }])).toBe('light');
		expect(newestOf([], 'bean_id', coffees)).toBe('aaa');
		expect(newestOf(roasts, 'profile_id', [])).toBe('');
	});

	it("words a cupping error with the page's buttons", () => {
		expect(
			reportError('cupping', 'no roast is cupped yet: `blank` makes a sheet to score on')
		).toBe('No roast is cupped yet: “A form to score on” makes a sheet to score on.');
		expect(
			reportError(
				'cupping',
				'none of these roasts is cupped yet: `blank` makes a sheet to score on'
			)
		).toMatch(/^None of the ticked roasts/);
		expect(reportError('cupping', 'no cupping on 2026-10-01')).toBe('no cupping on 2026-10-01');
		expect(reportError('roast', 'use `blank`')).toBe('use `blank`');
	});

	it('compares the newest roast with the one before it on its roaster', () => {
		const roasts = [
			{ id: 'n', device_id: 'esp' },
			{ id: 'v', device_id: 'virtual' },
			{ id: 'p', device_id: 'esp' }
		];
		expect(comparePair(roasts)).toEqual({ a: 'p', b: 'n' });
		// None after it on its roaster: the next one
		expect(comparePair(roasts, null, 'v')).toEqual({ a: 'p', b: 'v' });
		expect(comparePair(roasts, 'v', 'n')).toEqual({ a: 'v', b: 'n' });
		expect(comparePair([{ id: 'n', device_id: 'esp' }])).toEqual({ a: '', b: 'n' });
		expect(comparePair([])).toEqual({ a: '', b: '' });
		expect(compareParams('a1', 'b2')).toBe('roasts=a1%2Cb2');
		expect(compareFileName({ id: 'x', batch_number: 12 }, { id: 'abcdef123456' })).toBe(
			'compare-12-abcdef12.pdf'
		);
	});

	it('traces a roast back to its lot, or a coffee to every bag', () => {
		expect(traceParams('roast', 'r1', 'b1')).toBe('roast=r1');
		expect(traceParams('bean', 'r1', 'b1')).toBe('bean=b1');
		expect(traceParams('bean', 'r1', '')).toBe('');
	});
});
