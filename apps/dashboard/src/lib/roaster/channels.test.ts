import { describe, it, expect } from 'vitest';
import {
	candidateKeys,
	channelAxis,
	channelsLine,
	channelsProblem,
	channelValue,
	newChannel,
	tidyChannels
} from './channels.js';

const ch = (key: string, label = 'X', unit = 'C', max: number | null = null) => ({
	key,
	label,
	unit,
	max
});

describe('declared channels', () => {
	it('are checked as the server checks them', () => {
		expect(channelsProblem([ch('t3', 'Exhaust'), ch('burner_kpa', 'Gas', 'kPa', 4)])).toBeNull();
		expect(channelsProblem([ch('3t')])).toMatch(/key/);
		expect(channelsProblem([ch('beanTemp')])).toMatch(/field of its own/);
		expect(channelsProblem([ch('drum', 'Drum', '%')])).toMatch(/field of its own/);
		expect(channelsProblem([ch('t3'), ch('t3')])).toMatch(/twice/);
		expect(channelsProblem([ch('t3', '')])).toMatch(/label/);
		expect(channelsProblem([ch('t3', 'x', 'F')])).toMatch(/°C/);
		expect(channelsProblem([ch('t3', 'x', 'ninechars')])).toMatch(/unit/);
		expect(channelsProblem([ch('p', 'Gas', 'kPa', 0)])).toMatch(/above 0/);
		expect(channelsProblem(Array.from({ length: 9 }, (_, i) => ch(`t${i}`)))).toMatch(/At most 8/);
	});

	it('are tidied for the server: °C is C, a max only on its own scale', () => {
		expect(tidyChannels([ch(' t3 ', ' Exhaust ', '°C', 300), ch('gas', 'Gas', 'kPa', 4)])).toEqual([
			ch('t3', 'Exhaust', 'C', null),
			ch('gas', 'Gas', 'kPa', 4)
		]);
	});

	it('draw on the axis of their unit', () => {
		expect(channelAxis({ unit: 'C' })).toBe('temp');
		expect(channelAxis({ unit: '%' })).toBe('pct');
		expect(channelAxis({ unit: 'kPa' })).toBe('own');
	});

	it('offer the numeric keys a roaster sends that are not fields of their own', () => {
		expect(
			candidateKeys({
				beanTemp: 1,
				t3: 150,
				ambientTemp: 21,
				note: 'x',
				fan: 50,
				rssi: -60,
				'bad-key': 1
			})
		).toEqual(['ambientTemp', 'rssi', 't3']);
		expect(candidateKeys(null)).toEqual([]);
		expect(channelValue({ t3: 1.5 }, 't3')).toBe(1.5);
		expect(channelValue({ t3: 'x' }, 't3')).toBeNull();
	});

	it('read in a line, and start with a guessed unit', () => {
		expect(channelsLine([ch('t3', 'Exhaust'), ch('gas', 'Gas', 'kPa', 4)])).toBe(
			'Exhaust (t3, °C), Gas (gas, kPa, top 4)'
		);
		expect(channelsLine([])).toMatch(/None declared/);
		expect(newChannel('t3').unit).toBe('C');
		expect(newChannel('burner').unit).toBe('%');
		expect(newChannel('t3').label).toBe('T3');
	});
});
