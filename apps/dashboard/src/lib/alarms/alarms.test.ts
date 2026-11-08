import { describe, it, expect } from 'vitest';
import {
	normalizeRule,
	parseImport,
	describeActions,
	unitOf,
	suggestedAlarms,
	newAction,
	declaredChannels,
	channelRuleProblem,
	channelIsTemp
} from './alarms.js';

describe('alarm rules', () => {
	it('normalizes: legacy reference_event arms, empty fields dropped', () => {
		const r = normalizeRule({
			name: ' Low RoR ',
			condition_type: 'ror_below',
			threshold: 5,
			reference_event: 'first_crack_start',
			enabled: true,
			repeat: false,
			actions: []
		});
		expect(r).toEqual({
			name: 'Low RoR',
			condition_type: 'ror_below',
			threshold: 5,
			enabled: true,
			armed_after: 'first_crack_start'
		});
		const t = normalizeRule({
			name: 'Dev',
			condition_type: 'time_after_event',
			threshold: 90,
			enabled: true
		});
		expect(t.reference_event).toBe('charge');
		const rep = normalizeRule({
			name: 'x',
			condition_type: 'temp_above',
			threshold: 1,
			enabled: true,
			repeat: true,
			cooldown_seconds: 30
		});
		expect(rep.repeat).toBe(true);
		expect(rep.cooldown_seconds).toBe(30);
	});

	it('imports a list or {alarms: [...]} and rejects junk', () => {
		const list = JSON.stringify(suggestedAlarms());
		expect(parseImport(list)).toHaveLength(7);
		expect(parseImport(JSON.stringify({ alarms: suggestedAlarms() }))).toHaveLength(7);
		expect(
			parseImport('[{"name":"s","condition_type":"stall_ahead","threshold":120}]')[0].condition_type
		).toBe('stall_ahead');
		expect(() => parseImport('nope')).toThrow('not valid JSON');
		expect(() => parseImport('{"a":1}')).toThrow('list of alarms');
		expect(() => parseImport('[{"name":"x","condition_type":"moon","threshold":1}]')).toThrow(
			"unknown condition 'moon'"
		);
		expect(() => parseImport('[{"name":"x"}]')).toThrow('alarm 1');
	});

	it('describes actions and units', () => {
		expect(
			describeActions([
				{ type: 'heater_off' },
				{ type: 'fan', value: 100 },
				{ type: 'mark', event: 'drop' }
			])
		).toBe('heater off, fan 100 %, mark Drop');
		expect(describeActions([])).toBe('notify only');
		expect(unitOf('delta_below')).toBe('delta');
		expect(unitOf('et_above')).toBe('temp');
		expect(unitOf('no_telemetry')).toBe('seconds');
		expect(newAction('mode')).toEqual({ type: 'mode', mode: 'manual' });
	});

	it('channel rules name a declared channel, or say why they never fire', () => {
		const declared = declaredChannels([
			{ device_id: 'tc4', channels: [{ key: 't3', label: 'Exhaust', unit: 'C' }] },
			{
				device_id: 'drum2',
				channels: [
					{ key: 't3', label: 'Exhaust', unit: 'C' },
					{ key: 'gas', label: 'Gas', unit: 'kPa' }
				]
			},
			{ device_id: 'esp32_roaster_01' }
		]);
		expect(declared.map((d) => [d.key, d.devices])).toEqual([
			['gas', ['drum2']],
			['t3', ['tc4', 'drum2']]
		]);
		const rule = {
			name: 'Hot exhaust',
			condition_type: 'channel_above',
			threshold: 250,
			enabled: true,
			channel: 't3'
		};
		expect(channelRuleProblem(rule, declared)).toBeNull();
		expect(channelIsTemp(rule, declared)).toBe(true);
		expect(channelIsTemp({ ...rule, channel: 'gas' }, declared)).toBe(false);
		expect(channelRuleProblem({ ...rule, channel: 't9' }, declared)).toMatch(/never fires/);
		expect(channelRuleProblem({ ...rule, channel: null }, declared)).toMatch(/Pick a channel/);
		expect(channelRuleProblem({ ...rule, condition_type: 'temp_above' }, declared)).toBeNull();
		// The channel is kept on save, dropped from other conditions; import needs it.
		expect(normalizeRule(rule).channel).toBe('t3');
		expect(normalizeRule({ ...rule, condition_type: 'temp_above' }).channel).toBeUndefined();
		expect(() =>
			parseImport('[{"name":"x","condition_type":"channel_below","threshold":1}]')
		).toThrow('needs channel');
		expect(unitOf('channel_above')).toBe('channel');
	});
});
