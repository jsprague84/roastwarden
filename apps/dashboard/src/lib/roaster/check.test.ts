import { describe, it, expect } from 'vitest';
import {
	orderedLines,
	worst,
	statusClass,
	readingItems,
	writeSummary,
	checkLabel,
	verdictText
} from './check.js';

describe('roaster check', () => {
	it('orders the lines as the standard lists them', () => {
		const lines = [
			{ check: 'control', status: 'ok' as const, message: 'full' },
			{ check: 'new_one', status: 'warn' as const, message: '?' },
			{ check: 'rate', status: 'fail' as const, message: '0.2 Hz' },
			{ check: 'units', status: 'warn' as const, message: '°F?' }
		];
		expect(orderedLines(lines).map((l) => l.check)).toEqual([
			'rate',
			'units',
			'control',
			'new_one'
		]);
		expect(checkLabel('bt_et')).toBe('BT and ET');
		expect(checkLabel('new_one')).toBe('new one');
	});

	it('takes the worst status and colours it with tokens', () => {
		expect(worst([])).toBe('ok');
		expect(worst(['ok', 'warn', 'ok'])).toBe('warn');
		expect(worst(['warn', 'fail', 'ok'])).toBe('fail');
		expect(statusClass('ok')).toBe('text-go');
		expect(statusClass('warn')).toBe('text-highlight');
		expect(statusClass('fail')).toBe('text-destructive');
		expect(verdictText('fail')).toMatch(/not meet/);
	});

	it('lists the reading: temperatures, outputs, then extras', () => {
		const items = readingItems({ inlet: 250.5, drum: 55, bt: 120.1, heater: 60, et: 180 });
		expect(items.map((i) => i.key)).toEqual(['bt', 'et', 'heater', 'drum', 'inlet']);
		expect(items[0].kind).toBe('temp');
		expect(items[2].kind).toBe('pct');
		expect(items[4].kind).toBe('other');
		expect(readingItems(null)).toEqual([]);
	});

	it('sums up the write check', () => {
		const ok = {
			device_id: 'virtual_roaster',
			ok: true,
			steps: [
				{ step: 'heater 10 %', ok: true, at_ms: 0, message: 'sent' },
				{ step: 'emergency stop', ok: true, at_ms: 2300, message: 'heater 0' }
			]
		};
		expect(writeSummary(ok)).toBe('Passed in 2.3 s: the roaster obeys and stops.');
		const bad = {
			...ok,
			ok: false,
			steps: [{ step: 'echo', ok: false, at_ms: 5000, message: 'no echo in 5 s' }]
		};
		expect(writeSummary(bad)).toBe('Failed at “echo”: no echo in 5 s');
	});
});
