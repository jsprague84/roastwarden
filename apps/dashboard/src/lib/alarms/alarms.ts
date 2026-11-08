/**
 * Alarm rules as the server evaluates them (docs/alarms.md). Stored in the
 * `roast_alarms` setting; thresholds in °C, °C/min or seconds.
 */

export type ConditionType =
	| 'temp_above'
	| 'temp_below'
	| 'et_above'
	| 'et_below'
	| 'ror_above'
	| 'ror_below'
	| 'delta_above'
	| 'delta_below'
	| 'channel_above'
	| 'channel_below'
	| 'time_after_event'
	| 'no_telemetry'
	| 'crash_ahead'
	| 'flick_ahead'
	| 'stall_ahead';

/** How a threshold converts for display: `channel` follows the channel's unit (°C converts). */
export type UnitKind = 'temp' | 'delta' | 'ror' | 'seconds' | 'channel';

export const CONDITIONS: { value: ConditionType; label: string; unit: UnitKind; hint?: string }[] =
	[
		{ value: 'temp_above', label: 'BT above', unit: 'temp' },
		{ value: 'temp_below', label: 'BT below', unit: 'temp' },
		{ value: 'et_above', label: 'ET above', unit: 'temp' },
		{ value: 'et_below', label: 'ET below', unit: 'temp' },
		{ value: 'ror_above', label: 'RoR above', unit: 'ror' },
		{ value: 'ror_below', label: 'RoR below', unit: 'ror' },
		{
			value: 'delta_above',
			label: 'BT ahead of profile by',
			unit: 'delta',
			hint: 'BT − profile BT above this'
		},
		{
			value: 'delta_below',
			label: 'BT behind profile by',
			unit: 'delta',
			hint: 'BT − profile BT below minus this'
		},
		{
			value: 'channel_above',
			label: 'Channel above',
			unit: 'channel',
			hint: "a roaster's declared channel (Roasters → Setup → Channels)"
		},
		{
			value: 'channel_below',
			label: 'Channel below',
			unit: 'channel',
			hint: "a roaster's declared channel (Roasters → Setup → Channels)"
		},
		{ value: 'time_after_event', label: 'Time after landmark', unit: 'seconds' },
		{
			value: 'no_telemetry',
			label: 'No telemetry for',
			unit: 'seconds',
			hint: 'sensor, Wi-Fi or broker lost'
		},
		{
			value: 'crash_ahead',
			label: 'RoR crash ahead within',
			unit: 'seconds',
			hint: 'the RoR falling 8 °C/min or more in a minute, from the roaster’s own model or seen now'
		},
		{
			value: 'flick_ahead',
			label: 'RoR flick ahead within',
			unit: 'seconds',
			hint: 'the RoR rising again after first crack, from the roaster’s own model or seen now'
		},
		{
			value: 'stall_ahead',
			label: 'BT stall ahead within',
			unit: 'seconds',
			hint: 'the RoR at 1 °C/min or less before DROP, from the roaster’s own model or seen now'
		}
	];

/** The RoR watch's conditions (docs/prediction.md, "RoR warnings"): seconds ahead, 0 to 120. */
export const isAheadRule = (r: Pick<AlarmRule, 'condition_type'>) =>
	r.condition_type.endsWith('_ahead');

export const LANDMARKS: { value: string; label: string }[] = [
	{ value: 'charge', label: 'Charge' },
	{ value: 'turning_point', label: 'Turning point' },
	{ value: 'drying_end', label: 'Dry end' },
	{ value: 'first_crack_start', label: 'First crack' },
	{ value: 'first_crack_end', label: 'First crack end' },
	{ value: 'second_crack_start', label: 'Second crack' },
	{ value: 'drop', label: 'Drop' },
	{ value: 'cool_end', label: 'Cool end' }
];

export type AlarmAction =
	| { type: 'setpoint'; value: number }
	| { type: 'fan'; value: number }
	| { type: 'heater'; value: number }
	| { type: 'mode'; mode: 'auto' | 'manual' }
	| { type: 'heater_off' }
	| { type: 'emergency_stop' }
	| { type: 'mark'; event: string }
	| { type: 'replay_pause' }
	| { type: 'replay_stop' };

export const ACTIONS: { type: AlarmAction['type']; label: string }[] = [
	{ type: 'setpoint', label: 'Set setpoint' },
	{ type: 'fan', label: 'Set fan' },
	{ type: 'heater', label: 'Set heater' },
	{ type: 'mode', label: 'Set mode' },
	{ type: 'heater_off', label: 'Heater off' },
	{ type: 'emergency_stop', label: 'Emergency stop' },
	{ type: 'mark', label: 'Mark landmark' },
	{ type: 'replay_pause', label: 'Pause replay' },
	{ type: 'replay_stop', label: 'Stop replay' }
];

export interface AlarmRule {
	name: string;
	condition_type: ConditionType | string;
	threshold: number;
	/** time_after_event: the landmark the time counts from; others: arms the rule (legacy). */
	reference_event?: string | null;
	enabled: boolean;
	armed_after?: string | null;
	requires?: string | null;
	repeat?: boolean;
	cooldown_seconds?: number | null;
	actions?: AlarmAction[];
	/** channel_above / channel_below: the declared channel's key (docs/channels.md). */
	channel?: string | null;
}

/** A channel some roaster declares, and which ones (for the rule editor). */
export interface DeclaredChannel {
	key: string;
	label: string;
	unit: string;
	devices: string[];
}

/** The roasters' declared channels by key (rules are for every roaster). */
export function declaredChannels(
	setups: { device_id: string; channels?: { key: string; label: string; unit: string }[] }[]
): DeclaredChannel[] {
	const byKey = new Map<string, DeclaredChannel>();
	for (const s of setups) {
		for (const c of s.channels ?? []) {
			const d = byKey.get(c.key) ?? { key: c.key, label: c.label, unit: c.unit, devices: [] };
			d.devices.push(s.device_id);
			byKey.set(c.key, d);
		}
	}
	return [...byKey.values()].sort((a, b) => a.key.localeCompare(b.key));
}

export const isChannelRule = (r: Pick<AlarmRule, 'condition_type'>) =>
	r.condition_type === 'channel_above' || r.condition_type === 'channel_below';

/**
 * Why a channel rule may not fire, or null: it names no channel, or a channel no roaster
 * declares (the server never fires it on a roaster without that channel).
 */
export function channelRuleProblem(r: AlarmRule, declared: DeclaredChannel[]): string | null {
	if (!isChannelRule(r)) return null;
	if (!r.channel) return 'Pick a channel: without one this alarm is not saved.';
	const d = declared.find((c) => c.key === r.channel);
	if (!d)
		return `No roaster declares the channel ${r.channel}: this alarm never fires (Roasters → Setup → Channels).`;
	return null;
}

/** A channel rule's threshold in °C (a `C` channel), else in the channel's own unit. */
export function channelIsTemp(r: AlarmRule, declared: DeclaredChannel[]): boolean {
	return declared.find((c) => c.key === r.channel)?.unit === 'C';
}
export function unitOf(cond: string): UnitKind {
	return CONDITIONS.find((c) => c.value === cond)?.unit ?? 'seconds';
}

/** A new action of a type, with a sensible default value. */
export function newAction(type: AlarmAction['type']): AlarmAction {
	switch (type) {
		case 'setpoint':
			return { type, value: 200 };
		case 'fan':
			return { type, value: 100 };
		case 'heater':
			return { type, value: 0 };
		case 'mode':
			return { type, mode: 'manual' };
		case 'mark':
			return { type, event: 'drop' };
		default:
			return { type } as AlarmAction;
	}
}

/**
 * Clean a rule for saving: drop empty optional fields, and move a legacy
 * `reference_event` used for arming onto `armed_after`.
 */
export function normalizeRule(r: AlarmRule): AlarmRule {
	const out: AlarmRule = {
		name: r.name.trim() || 'Alarm',
		condition_type: r.condition_type,
		threshold: Number(r.threshold),
		enabled: r.enabled !== false
	};
	if (r.condition_type === 'time_after_event') {
		out.reference_event = r.reference_event || 'charge';
		if (r.armed_after) out.armed_after = r.armed_after;
	} else {
		const arm = r.armed_after || r.reference_event;
		if (arm) out.armed_after = arm;
	}
	if (isChannelRule(r) && r.channel) out.channel = r.channel;
	if (r.requires) out.requires = r.requires;
	if (r.repeat) {
		out.repeat = true;
		if (r.cooldown_seconds != null && Number.isFinite(Number(r.cooldown_seconds)))
			out.cooldown_seconds = Number(r.cooldown_seconds);
	}
	if (r.actions?.length) out.actions = r.actions.map((a) => ({ ...a }));
	return out;
}

/** One-line summary: "BT above 230 °C → heater off, fan 100 %". */
export function describeActions(actions: AlarmAction[] | undefined): string {
	if (!actions?.length) return 'notify only';
	return actions
		.map((a) => {
			switch (a.type) {
				case 'setpoint':
					return `setpoint ${a.value} °C`;
				case 'fan':
					return `fan ${a.value} %`;
				case 'heater':
					return `heater ${a.value} %`;
				case 'mode':
					return `mode ${a.mode}`;
				case 'mark':
					return `mark ${LANDMARKS.find((l) => l.value === a.event)?.label ?? a.event}`;
				default:
					return ACTIONS.find((x) => x.type === a.type)?.label.toLowerCase() ?? a.type;
			}
		})
		.join(', ');
}

/** Parse an imported alarm list; throws with a readable message. */
export function parseImport(text: string): AlarmRule[] {
	let data: unknown;
	try {
		data = JSON.parse(text);
	} catch {
		throw new Error('not valid JSON');
	}
	const list = Array.isArray(data) ? data : (data as { alarms?: unknown })?.alarms;
	if (!Array.isArray(list)) throw new Error('expected a list of alarms');
	return list.map((x, i) => {
		const r = x as Partial<AlarmRule>;
		if (
			typeof r.name !== 'string' ||
			typeof r.condition_type !== 'string' ||
			typeof r.threshold !== 'number'
		)
			throw new Error(`alarm ${i + 1}: needs name, condition_type and threshold`);
		if (!CONDITIONS.some((c) => c.value === r.condition_type))
			throw new Error(`alarm ${i + 1}: unknown condition '${r.condition_type}'`);
		if (isChannelRule(r as AlarmRule) && typeof r.channel !== 'string')
			throw new Error(`alarm ${i + 1}: ${r.condition_type} needs channel`);
		return normalizeRule({ enabled: true, ...r } as AlarmRule);
	});
}

/**
 * Starting rules for a fluid bed: a hard ceiling, a dead-sensor stop, a DROP follow-up, a
 * behind-profile warning, and the RoR watch's crash, flick and stall warnings (notify only).
 */
export function suggestedAlarms(): AlarmRule[] {
	return [
		{
			name: 'Too hot: heater off',
			condition_type: 'temp_above',
			threshold: 235,
			enabled: true,
			actions: [{ type: 'heater_off' }, { type: 'fan', value: 100 }]
		},
		{
			name: 'Telemetry lost: heater off',
			condition_type: 'no_telemetry',
			threshold: 15,
			enabled: true,
			actions: [{ type: 'heater_off' }]
		},
		{
			name: 'After drop: cool',
			condition_type: 'time_after_event',
			threshold: 0,
			reference_event: 'drop',
			enabled: true,
			actions: [{ type: 'replay_stop' }, { type: 'heater_off' }, { type: 'fan', value: 100 }]
		},
		{
			name: 'Behind profile',
			condition_type: 'delta_below',
			threshold: -8,
			armed_after: 'turning_point',
			repeat: true,
			cooldown_seconds: 120,
			enabled: true
		},
		{ name: 'RoR crash ahead', condition_type: 'crash_ahead', threshold: 60, enabled: true },
		{ name: 'RoR flick ahead', condition_type: 'flick_ahead', threshold: 60, enabled: true },
		{ name: 'BT stall ahead', condition_type: 'stall_ahead', threshold: 120, enabled: true }
	];
}
