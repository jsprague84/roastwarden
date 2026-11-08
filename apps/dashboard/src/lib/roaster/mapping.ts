/**
 * A connection's mapping as the Mapping editor edits it (docs/ingest-mapping.md, docs/add-roaster.md):
 * the device's own field names, unit, columns, topics and command templates, as rows a form can
 * hold, and back to the connection config the server reads. The server checks the same rules
 * and more when it saves (a 400 says what is wrong); `mappingProblems` only catches the obvious
 * early, in the same words.
 */
import { isInput } from '#lib/roaster/phidget.js';

/** The connections a mapping applies to; Modbus maps registers by role instead. A Phidget's
 * fields are the JSON kind with the board's inputs (`ch0`–`ch7`) as paths, in °C, no commands. */
export type MappingProtocol = 'mqtt' | 'websocket' | 'serial_tc4' | 'phidget22';

export function hasMapping(protocol: string): protocol is MappingProtocol {
	return (
		protocol === 'mqtt' ||
		protocol === 'websocket' ||
		protocol === 'serial_tc4' ||
		protocol === 'phidget22'
	);
}

/** The roles a reading can have (docs/ingest-mapping.md, "Roles"); any other key is an extra channel. */
export const READ_ROLES = [
	'bt',
	'et',
	'ambient',
	'ror',
	'heater',
	'heater_applied',
	'fan',
	'drum',
	'damper',
	'setpoint',
	'mode',
	'heater_enable'
] as const;
export type ReadRole = (typeof READ_ROLES)[number];

export const READ_ROLE_LABEL: Record<ReadRole, string> = {
	bt: 'BT',
	et: 'ET',
	ambient: 'Ambient',
	ror: 'RoR (the device’s)',
	heater: 'Heater / burner %',
	heater_applied: 'Heater applied %',
	fan: 'Fan / air %',
	drum: 'Drum %',
	damper: 'Damper %',
	setpoint: 'Setpoint',
	mode: 'Mode (0 manual, 1 auto)',
	heater_enable: 'Heater on (0 / 1)'
};

/** The commands a template may be given (the contract's PID gains and lease take none). */
export const COMMAND_KEYS = [
	'heater',
	'fan',
	'drum',
	'damper',
	'setpoint',
	'mode',
	'heater_enable',
	'emergency_stop'
] as const;
type CommandKey = (typeof COMMAND_KEYS)[number];

/** Telemetry fields Roastwarden fills itself: no extra channel may take one of these names. */
const RESERVED = [
	'beanTemp',
	'envTemp',
	'ambientTemp',
	'rateOfRise',
	'rateOfRiseEt',
	'deviceRateOfRise',
	'heater',
	'heaterApplied',
	'fan',
	'drum',
	'damper',
	'setpoint',
	'controlMode',
	'heaterEnable',
	'timestamp',
	'uptime',
	'uptimeMs',
	'emergencyStop'
];

const KEY_RE = /^[A-Za-z][A-Za-z0-9_]{0,31}$/;

export function isRole(key: string): key is ReadRole {
	return (READ_ROLES as readonly string[]).includes(key);
}

type Units = 'C' | 'F';

/** One mapped field. Numbers are text so an empty box means the default. */
export interface FieldRow {
	/** A role (`bt`) or an extra channel's name (`exhaust`). */
	key: string;
	/** `data.BT`, `probes[0].value`, `$` (the whole payload). */
	path: string;
	scale: string;
	offset: string;
	/** An extra channel's own unit; roles follow the mapping's units. */
	unit: '' | Units;
	/** MQTT: the field's own topic (ESPHome style); empty: the telemetry topic. */
	topic: string;
}

export interface TemplateRow {
	key: CommandKey | string;
	/** MQTT only; empty: `<prefix>/control/<command>`. */
	topic: string;
	/** `{value}` is the command's value; empty: the value alone. */
	payload: string;
	scale: string;
	decimals: string;
}

export interface MappingForm {
	units: Units;
	fields: FieldRow[];
	/** TC4: the reply's columns by position; '' skips one. */
	columns: string[];
	templates: TemplateRow[];
	/** MQTT: empty is the default `roaster/<id>`. */
	topicPrefix: string;
	/** MQTT: empty is `<prefix>/telemetry`. */
	telemetryTopic: string;
}

export function emptyField(key = ''): FieldRow {
	return { key, path: '', scale: '', offset: '', unit: '', topic: '' };
}

export function emptyTemplate(key = ''): TemplateRow {
	return { key, topic: '', payload: '', scale: '', decimals: '' };
}

const text = (v: unknown): string => (v === undefined || v === null ? '' : String(v));
const obj = (v: unknown): Record<string, unknown> | null =>
	v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;

/** The editor's rows from a saved connection config. */
export function mappingFromConfig(
	protocol: MappingProtocol,
	config: Record<string, unknown>
): MappingForm {
	const ingest = obj(config.ingest);
	const form: MappingForm = {
		units: ingest?.units === 'F' ? 'F' : 'C',
		fields: [],
		columns: [],
		templates: [],
		topicPrefix: protocol === 'mqtt' ? text(config.topic_prefix) : '',
		telemetryTopic: protocol === 'mqtt' ? text(config.telemetry_topic) : ''
	};
	if (Array.isArray(ingest?.columns)) form.columns = ingest.columns.map((c) => text(c));
	// A Phidget with no mapping reads BT ch0, ET ch1: the editor starts from what is in effect
	const fieldSpecs =
		obj(ingest?.fields) ?? (protocol === 'phidget22' ? { bt: 'ch0', et: 'ch1' } : {});
	for (const [key, spec] of Object.entries(fieldSpecs)) {
		const row = emptyField(key);
		if (typeof spec === 'string') row.path = spec;
		else {
			const o = obj(spec) ?? {};
			row.path = text(o.path);
			row.scale = text(o.scale);
			row.offset = text(o.offset);
			row.unit = o.unit === 'C' || o.unit === 'F' ? o.unit : '';
			row.topic = text(o.topic);
		}
		form.fields.push(row);
	}
	for (const [key, spec] of Object.entries(obj(config.commands) ?? {})) {
		const o = obj(spec) ?? {};
		form.templates.push({
			key,
			topic: text(o.topic),
			payload: text(o.payload),
			scale: text(o.scale),
			decimals: text(o.decimals)
		});
	}
	return form;
}

const num = (s: string): number | undefined => {
	const t = s.trim();
	if (t === '') return undefined;
	const n = Number(t);
	return Number.isFinite(n) ? n : NaN;
};

/** The `ingest` the form describes, or null for none (the device sends Roastwarden's own JSON). */
export function ingestOf(
	protocol: MappingProtocol,
	form: MappingForm
): Record<string, unknown> | null {
	if (protocol === 'serial_tc4') {
		const cols = form.columns.map((c) => c.trim());
		const named = cols.some((c) => c !== '');
		if (!named && form.units === 'C') return null;
		const out: Record<string, unknown> = {};
		if (form.units === 'F') out.units = 'F';
		if (named) out.columns = cols;
		return out;
	}
	const rows = form.fields.filter((f) => f.key.trim() !== '');
	if (rows.length === 0) return null;
	const fields: Record<string, unknown> = {};
	for (const f of rows) {
		const key = f.key.trim();
		const scale = num(f.scale);
		const offset = num(f.offset);
		const topic = protocol === 'mqtt' ? f.topic.trim() : '';
		// A Phidget reports °C: no field takes a unit
		const unit = isRole(key) || protocol === 'phidget22' ? '' : f.unit;
		const plain = scale === undefined && offset === undefined && !unit && !topic;
		if (plain) {
			fields[key] = f.path.trim();
			continue;
		}
		const o: Record<string, unknown> = {};
		if (f.path.trim() !== '' || !topic) o.path = f.path.trim();
		if (scale !== undefined) o.scale = scale;
		if (offset !== undefined) o.offset = offset;
		if (unit) o.unit = unit;
		if (topic) o.topic = topic;
		fields[key] = o;
	}
	const out: Record<string, unknown> = { fields };
	if (form.units === 'F' && protocol !== 'phidget22') out.units = 'F';
	return out;
}

/** The `commands` the form describes, or null for none (today's syntax for every command). */
export function commandsOf(
	protocol: MappingProtocol,
	form: MappingForm
): Record<string, unknown> | null {
	// A Phidget reads temperatures only
	if (protocol === 'phidget22') return null;
	const rows = form.templates.filter((t) => t.key.trim() !== '');
	if (rows.length === 0) return null;
	const out: Record<string, unknown> = {};
	for (const t of rows) {
		const o: Record<string, unknown> = {};
		if (protocol === 'mqtt' && t.topic.trim()) o.topic = t.topic.trim();
		if (t.payload !== '') o.payload = t.payload;
		const scale = num(t.scale);
		if (scale !== undefined) o.scale = scale;
		const dec = num(t.decimals);
		if (dec !== undefined) o.decimals = dec;
		out[t.key.trim()] = o;
	}
	return out;
}

/** The connection config with the form's mapping in place of the saved one; the rest is kept. */
export function configWithMapping(
	protocol: MappingProtocol,
	config: Record<string, unknown>,
	form: MappingForm
): Record<string, unknown> {
	const out: Record<string, unknown> = { ...config };
	delete out.ingest;
	delete out.commands;
	const ingest = ingestOf(protocol, form);
	const commands = commandsOf(protocol, form);
	if (ingest) out.ingest = ingest;
	if (commands) out.commands = commands;
	if (protocol === 'mqtt') {
		delete out.topic_prefix;
		delete out.telemetry_topic;
		if (form.topicPrefix.trim()) out.topic_prefix = form.topicPrefix.trim().replace(/\/+$/, '');
		if (form.telemetryTopic.trim()) out.telemetry_topic = form.telemetryTopic.trim();
	}
	return out;
}

function topicProblem(t: string, what: string): string | null {
	if (t === '') return null;
	if (t.includes('+') || t.includes('#'))
		return `${what}: '${t}' has a wildcard (+ or #); name one topic`;
	if (t.length > 256) return `${what}: a topic is 1 to 256 characters`;
	return null;
}

/** What is plainly wrong with the form, in the server's words; empty when it can be tried. */
export function mappingProblems(protocol: MappingProtocol, form: MappingForm): string[] {
	const out: string[] = [];
	if (protocol === 'serial_tc4') {
		const texts = form.columns.map((c) => c.trim()).filter((c) => c !== '');
		// An extra column may say its unit: exhaust:F (C, F or none)
		const cols = texts.map((c) => c.split(':')[0].trim());
		if (cols.length > 0) {
			if (!cols.includes('bt') && !cols.includes('et'))
				out.push('the columns must name bt or et (or both)');
			const dup = cols.find((c, i) => cols.indexOf(c) !== i);
			if (dup) out.push(`'${dup}' is in two columns`);
			for (const c of cols)
				if (!isRole(c) && (!KEY_RE.test(c) || RESERVED.includes(c)))
					out.push(`column '${c}' is not a role or a name`);
			for (const [i, t] of texts.entries()) {
				if (!t.includes(':')) continue;
				const unit = t.slice(t.indexOf(':') + 1).trim();
				if (!['C', 'F', 'none'].includes(unit))
					out.push(`'${t}': the unit after ':' is C, F or none`);
				else if (isRole(cols[i]))
					out.push(`'${t}': a unit is for an extra column; ${cols[i]} follows the device's unit`);
			}
			if (form.columns.length > 32) out.push('at most 32 columns');
		}
	} else {
		const rows = form.fields.filter((f) => f.key.trim() !== '');
		const keys = rows.map((f) => f.key.trim());
		if (rows.length > 0 && !keys.includes('bt') && !keys.includes('et'))
			out.push('map bt or et (or both)');
		if (rows.length === 0 && form.units === 'F' && protocol !== 'phidget22')
			out.push('°F needs at least bt or et mapped');
		if (rows.length > 32) out.push('at most 32 fields');
		const dup = keys.find((k, i) => keys.indexOf(k) !== i);
		if (dup) out.push(`'${dup}' is mapped twice`);
		for (const f of rows) {
			const k = f.key.trim();
			if (!isRole(k) && (!KEY_RE.test(k) || RESERVED.includes(k)))
				out.push(`'${k}' is not a role, and not a free name (a letter, then letters, digits or _)`);
			const scale = num(f.scale);
			if (Number.isNaN(scale) || scale === 0) out.push(`${k}: scale must be a number other than 0`);
			if (Number.isNaN(num(f.offset))) out.push(`${k}: offset must be a number`);
			const tp = protocol === 'mqtt' ? topicProblem(f.topic.trim(), k) : null;
			if (tp) out.push(tp);
			if (protocol === 'phidget22' && !isInput(f.path))
				out.push(`${k}: '${f.path.trim()}' is not a board input: ch0 to ch7`);
		}
	}
	if (protocol === 'phidget22' && form.templates.some((t) => t.key.trim() !== ''))
		out.push('a Phidget connection reads temperatures only: it takes no commands');
	const tkeys = form.templates.map((t) => t.key.trim()).filter((k) => k !== '');
	for (const k of tkeys)
		if (!(COMMAND_KEYS as readonly string[]).includes(k))
			out.push(`no template for '${k}': ${COMMAND_KEYS.join(', ')}`);
	const tdup = tkeys.find((k, i) => tkeys.indexOf(k) !== i);
	if (tdup) out.push(`two templates for ${tdup}`);
	for (const t of form.templates.filter((t) => t.key.trim() !== '')) {
		if (t.payload.length > 256) out.push(`${t.key}: the payload is at most 256 characters`);
		const s = num(t.scale);
		if (Number.isNaN(s) || s === 0) out.push(`${t.key}: scale must be a number other than 0`);
		const d = num(t.decimals);
		if (d !== undefined && (Number.isNaN(d) || !Number.isInteger(d) || d < 0 || d > 3))
			out.push(`${t.key}: decimals 0 to 3`);
		const tp = protocol === 'mqtt' ? topicProblem(t.topic.trim(), t.key) : null;
		if (tp) out.push(tp);
	}
	if (protocol === 'mqtt') {
		for (const [t, what] of [
			[form.topicPrefix.trim(), 'prefix'],
			[form.telemetryTopic.trim(), 'telemetry topic']
		] as const) {
			const tp = topicProblem(t, what);
			if (tp) out.push(tp);
		}
	}
	return out;
}

/** The field a key becomes, for the preview's header: `bt` → `beanTemp`, an extra keeps its name. */
const ROLE_FIELD: Record<ReadRole, string> = {
	bt: 'beanTemp',
	et: 'envTemp',
	ambient: 'ambientTemp',
	ror: 'rateOfRise',
	heater: 'heater',
	heater_applied: 'heaterApplied',
	fan: 'fan',
	drum: 'drum',
	damper: 'damper',
	setpoint: 'setpoint',
	mode: 'controlMode',
	heater_enable: 'heaterEnable'
};

export function fieldOf(key: string): string {
	return isRole(key) ? ROLE_FIELD[key] : key;
}

/** A raw payload made readable: JSON pretty-printed, anything else as it came. */
export function prettyPayload(payload: string): string {
	try {
		return JSON.stringify(JSON.parse(payload), null, 2);
	} catch {
		return payload;
	}
}
