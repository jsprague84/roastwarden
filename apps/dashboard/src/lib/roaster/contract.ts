/** The device contract in words (docs/control.md, "Device contract"). */
import type { ContractView } from '#lib/api/roasters.js';
import type { Gains } from '#lib/api/tuning.js';

const CAPS: [keyof ContractView['caps'], string][] = [
	['pid', 'onboard PID'],
	['ff', 'feedforward'],
	['lease', 'control lease'],
	['tpc', 'time-proportioned heater'],
	['ror', 'RoR limiter']
];

function gainsText(g: Gains | null | undefined): string {
	if (!g) return 'none';
	return `Kp ${g.kp} · Ki ${g.ki} · Kd ${g.kd}`;
}

export interface ContractLines {
	/** The required readings, or the ones missing */
	reads: { ok: boolean; text: string };
	caps: string;
	gains: { ok: boolean | null; text: string };
	boot: string | null;
}

export function contractLines(c: ContractView): ContractLines {
	const missing = (['bt', 'et', 'heater', 'fan'] as const).filter((k) => !c.reads[k]);
	const names = { bt: 'BT', et: 'ET', heater: 'heater', fan: 'fan' };
	const caps = CAPS.filter(([k]) => c.caps[k]).map(([, n]) => n);
	const g = c.gains;
	const gains =
		g.matches === true
			? { ok: true, text: `${gainsText(g.reported)}: the ones the server holds` }
			: g.matches === false
				? {
						ok: false,
						text: `reports ${gainsText(g.reported)}, the server holds ${gainsText(g.held)}`
					}
				: g.held
					? { ok: null, text: `the server holds ${gainsText(g.held)}; the roaster reports none` }
					: { ok: null, text: `reports ${gainsText(g.reported)}; the server holds none for it` };
	const b = g.last_boot;
	const boot = b
		? `${b.why === 'reboot' ? 'After its last reboot' : 'At its first sample since the server started'} (${new Date(b.at).toLocaleString()}) it had ${gainsText(b.reported)}; ${b.sent ? `the server sent ${gainsText(b.sent)}` : 'nothing was sent'}.`
		: null;
	return {
		reads: missing.length
			? { ok: false, text: `its latest sample lacks ${missing.map((k) => names[k]).join(', ')}` }
			: { ok: true, text: 'sends BT and ET, reports heater and fan' },
		caps: `${caps.length ? caps.join(', ') : 'none'} (${c.caps.reported ? 'as it reports them' : 'inferred from its telemetry'})`,
		gains,
		boot
	};
}
