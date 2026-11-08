/**
 * A serial TC4 / aArtisan connection's settings (docs/hardware-integration-guide.md,
 * "Serial Connection"): the `CHAN` mapping the server sends, how the device lays out
 * its answer to `READ`, and which output drives the fan. The server's side is
 * crates/server/src/transports/tc4.rs; a setting left out of the config keeps its old meaning.
 */
import type { Help } from '#lib/constants/help.js';

export type ReplyFormat = 'fixed' | 'active';
export type FanOutput = 'io3' | 'ot2' | 'dcfan' | 'none';

export interface Tc4Settings {
	port: string;
	baud: number;
	poll_interval_ms: number;
	/** Physical input (1–4, 0 = off) of logical channel 1, read as ET. */
	et_channel: number;
	/** Physical input of logical channel 2, read as BT. */
	bt_channel: number;
	/** Physical input of logical channel 3, carried as `t3`. */
	t3_channel: number;
	/** Physical input of logical channel 4, carried as `t4`. */
	t4_channel: number;
	reply_format: ReplyFormat;
	fan_output: FanOutput;
}

/**
 * A new connection's settings. Stock aArtisan prints only the channels `CHAN` turned on,
 * so a new connection expects that; a config with no `reply_format` (made before it
 * existed) is read the fixed way, as it always was.
 */
export function defaultTc4Settings(): Tc4Settings {
	return {
		port: '/dev/ttyUSB0',
		baud: 115200,
		poll_interval_ms: 1000,
		et_channel: 1,
		bt_channel: 2,
		t3_channel: 0,
		t4_channel: 0,
		reply_format: 'active',
		fan_output: 'io3'
	};
}

export const REPLY_FORMATS: ReplyFormat[] = ['active', 'fixed'];
export const REPLY_LABEL: Record<ReplyFormat, string> = {
	active: 'Stock aArtisan (active channels only)',
	fixed: 'Fixed four channels (Roastwarden firmware)'
};
export const REPLY_HELP: Record<ReplyFormat, Help> = {
	active: {
		line: 'The answer to READ has ambient, then only the channels turned on, then heater, fan and SV.',
		more: 'What stock aArtisan and aArtisanQ_PID print: "ambient,ET,BT,heater,fan" with two channels on, SV added while the PID runs (aArtisanQ_PID always prints it, 0 when off). Pick this for a TC4 shield or any board that runs aArtisan.'
	},
	fixed: {
		line: 'The answer to READ always has ambient and all four channels (-1 for none), then heater, fan, SV.',
		more: 'What the Roastwarden ESP32 firmware, the example in the hardware guide and the mock device print: "ambient,T1,T2,T3,T4,heater,fan,sv[,pid_on,heater_en]". A connection made before this setting existed is read this way.'
	}
};

export const FAN_OUTPUTS: FanOutput[] = ['io3', 'ot2', 'dcfan', 'none'];
export const FAN_LABEL: Record<FanOutput, string> = {
	io3: 'IO3',
	ot2: 'OT2',
	dcfan: 'DCFAN (IO3, slew-limited)',
	none: 'None'
};
export const FAN_HELP: Help = {
	line: 'The output a fan command goes to.',
	more: "IO3 is aArtisan's fan (IO3;n). OT2 is the second SSR or phase-angle output, aArtisanQ_PID's fan. DCFAN drives IO3 but rises at most 25 points a second, which some Hottops need. None: the server drives no fan and refuses fan commands. Emergency stop turns the heat off and this output to 100 %."
};

const CHANNELS = [
	{ key: 'et_channel', name: 'ET' },
	{ key: 'bt_channel', name: 'BT' },
	{ key: 't3_channel', name: 'T3' },
	{ key: 't4_channel', name: 'T4' }
] as const;

const isOn = (c: number) => Number.isInteger(c) && c >= 1 && c <= 4;

/** The `CHAN;ijkl` line the server sends at open. */
export function chanLine(s: Tc4Settings): string {
	return (
		'CHAN;' +
		CHANNELS.map((c) => String(Math.min(9, Math.max(0, Math.trunc(s[c.key] || 0))))).join('')
	);
}

/** What the server expects back from `READ`, column by column. */
export function replyShape(s: Tc4Settings): string {
	const temps =
		s.reply_format === 'fixed'
			? ['T1 (ET)', 'T2 (BT)', 'T3', 'T4']
			: CHANNELS.filter((c) => isOn(s[c.key])).map((c) => c.name);
	return ['ambient', ...temps, 'heater', 'fan', '[SV]'].join(',');
}

/** What is wrong with the settings, in words; empty when they can be saved. */
export function tc4Problems(s: Tc4Settings): string[] {
	const out: string[] = [];
	if (s.port.trim() === '') out.push('A serial port is needed.');
	if (!(s.baud > 0)) out.push('The baud rate must be above 0.');
	for (const c of CHANNELS) {
		const v = s[c.key];
		if (!Number.isInteger(v) || v < 0 || v > 4) out.push(`${c.name} channel must be 0 (off) to 4.`);
	}
	if (!isOn(s.et_channel) && !isOn(s.bt_channel)) out.push('ET or BT needs a channel.');
	const used = CHANNELS.map((c) => s[c.key]).filter(isOn);
	if (new Set(used).size !== used.length)
		out.push('Each physical input can feed one channel only.');
	return out;
}

/** The connection's JSON config, as the server stores it. */
export function tc4Config(s: Tc4Settings): Record<string, unknown> {
	return {
		port: s.port.trim(),
		baud: s.baud,
		poll_interval_ms: s.poll_interval_ms,
		et_channel: s.et_channel,
		bt_channel: s.bt_channel,
		t3_channel: s.t3_channel,
		t4_channel: s.t4_channel,
		reply_format: s.reply_format,
		fan_output: s.fan_output
	};
}

/** A config as the form's settings, reading a missing setting as the server does. */
export function tc4SettingsOf(cfg: Record<string, unknown>): Tc4Settings {
	const n = (k: string, d: number) => (typeof cfg[k] === 'number' ? (cfg[k] as number) : d);
	return {
		port: String(cfg.port ?? ''),
		baud: n('baud', 115200),
		poll_interval_ms: n('poll_interval_ms', 1000),
		et_channel: n('et_channel', 1),
		bt_channel: n('bt_channel', 2),
		t3_channel: n('t3_channel', 0),
		t4_channel: n('t4_channel', 0),
		reply_format: cfg.reply_format === 'active' ? 'active' : 'fixed',
		fan_output: FAN_OUTPUTS.includes(cfg.fan_output as FanOutput)
			? (cfg.fan_output as FanOutput)
			: 'io3'
	};
}

/** One line for a saved connection, reading a missing setting as the server does. */
export function tc4Summary(cfg: Record<string, unknown>): string {
	const s = tc4SettingsOf(cfg);
	const reply = s.reply_format === 'active' ? 'aArtisan replies' : 'fixed replies';
	return `${s.port} @ ${s.baud} · ${chanLine(s)} · ${reply} · fan ${FAN_LABEL[s.fan_output].split(' ')[0]}`;
}
