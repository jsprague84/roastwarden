/**
 * In-place explanations of every mode (docs/ui-structure.md). `line` sits
 * under the control; `more` opens with the "?" next to it. Wording follows
 * the server docs: control.md, replay.md, fan-assist.md, autotune-2.md.
 */
export interface Help {
	line: string;
	more?: string;
}

export const HELP = {
	tuneServer: {
		line: 'The server runs the test: it heats to the level, finds the heater % that holds it, then swings the heater around it and fits a model to what BT did.',
		more: 'It works on any roaster that takes a heater % and reports BT, the virtual roaster too. The roaster stays in manual, under a control lease: if the server goes quiet the roaster cuts its own heater. Load beans as for a roast (junk beans are fine) and test once per fan speed you roast at, near the BT that fan is used at. About 5 minutes after the heat-up.'
	},
	tuneModel: {
		line: 'What the roaster acts like near the test level, fitted to the record.',
		more: 'The test fits integrator, first- and second-order models with dead time and keeps the one that explains the record best. On a lag-dominant roaster a short test pins down only how fast BT moves per % of heater and the dead time, so the PI is designed on an integrator: that is all a PI needs. Where the dashed model follows the measured BT, the designs can be trusted.'
	},
	tuneKnob: {
		line: 'How fast the loop is asked to be, as a multiple of the dead time.',
		more: 'Tight follows the profile closest and moves the heater most; smooth is calmer and tolerates more change in the roaster (more beans, another fan). 1.5 is a good start. Ms says how much margin there is: 1.2–1.4 robust, up to 1.8 balanced, above 2 fragile.'
	},
	manual: {
		line: 'You set the heater %. The setpoint is not used.',
		more: 'The roaster applies the heater slider directly. Use it for preheat, a hand-driven roast, or when you want full control. Switching to Auto starts the PID from the current heater output, so there is no jump.'
	},
	auto: {
		line: "The roaster's PID moves the heater to hold BT at the setpoint.",
		more: 'The heater slider shows what the PID is doing and cannot be moved. Change the setpoint to change the target. The gains come from the gain schedule when it is on (picked by fan speed), else the stored gains (Roasters → this roaster → Control).'
	},
	setpoint: {
		line: 'The BT the PID holds in Auto.',
		more: 'In Manual the setpoint is stored but nothing acts on it. During a live replay in Setpoint mode the profile sets it every second; typing a setpoint pauses that channel.'
	},
	heaterEnable: {
		line: 'Master switch: off cuts heater power in any mode.',
		more: 'The firmware also refuses heat below 40 % fan. DROP and Complete switch the heater off for you.'
	},
	fanAssist: {
		line: 'Lowers the fan when the heater is pinned; the fan slider becomes the ceiling.',
		more: 'A heat-limited roaster cannot carry BT past ~185 °C at full fan. With assist on, while the PID heater sits above 92 % (or BT falls behind its target) the server takes air away, never below the floor (70 % by default), and gives it back once the heater has room. It only acts in Auto or a closed-loop replay, once BT passes 150 °C. Setting the fan by hand sets a new ceiling. Settings: Roasters → this roaster → Control.'
	},
	ghost: {
		line: 'Ghost computes what the replay would send and shows it; nothing is sent.',
		more: 'Use it to check timing and alignment against a live roast you drive yourself. "Engage" switches the same run to live: the roaster mode is set and the current values are sent.'
	},
	live: {
		line: 'Live: the profile drives the roaster every second.',
		more: 'Touching the setpoint, heater, fan or mode pauses that channel only: the profile keeps advancing and Resume sends its current value. Emergency stop, DROP and completing the roast stop the replay. Stopping never changes the outputs; they stay where the replay left them.'
	},
	modeSetpoint: {
		line: 'Follows the profile BT curve with the PID (closed loop).',
		more: 'The setpoint is the profile BT a few seconds ahead (the lookahead in Settings → Roasting defaults), so the PID starts turning before the curve does.'
	},
	modeFeedforward: {
		line: "Plays the profile's heater % open loop; BT is not corrected.",
		more: 'Repeats a recorded roast exactly. Needs a profile with heater data. Good for comparing beans under identical heat; bad if the room or batch size differs.'
	},
	modeFeedforwardPid: {
		line: "The profile's heater % plus PID correction on BT.",
		more: 'The recorded heater does most of the work and the PID trims the error, so it tracks closely without the PID having to anticipate. Needs a profile with heater data.'
	},
	loop: {
		line: 'Where the BT PID runs.',
		more: 'On the roaster (default when it supports it): fast, next to the heater, immune to network jitter. On the server: works with any roaster that takes a heater % (TC4, Modbus); gains in the server_pid setting. Auto picks the roaster when it can.'
	},
	trigger: {
		line: "When the profile's step changes fire.",
		more: "Each event's own: whatever the profile recorded (time or BT). By time: at the profile time. By BT: when BT reaches the event's temperature after the turning point, so a slow or fast batch still gets its changes at the same bean temperature."
	},
	align: {
		line: 'Which landmark lines the profile up with this roast.',
		more: 'CHARGE (default) starts the profile when you charge. TP, DRY or FC shift the profile once that landmark is marked live, so later stages line up even if the start ran long or short. Until then it runs from CHARGE.'
	},
	fanChannel: {
		line: "Replay the profile's fan.",
		more: 'With fan assist on, the profile fan is the ceiling and the assist trims below it. Off: the fan stays where you set it.'
	},
	channelPause: {
		line: 'Pause hands a channel back to you; the profile keeps running underneath.'
	},
	charge: {
		line: 'Mark the moment the beans go in. Profile time starts here.'
	},
	chargeVirtual: {
		line: 'Loads cold beans into the simulator and marks CHARGE.'
	},
	drop: {
		line: 'Marks DROP and switches the heater off.'
	},
	dropVirtual: {
		line: 'Marks DROP, switches the heater off and dumps the simulated beans.'
	},
	gainSchedule: {
		line: 'Picks PID gains by fan speed when on; otherwise the stored gains are used.',
		more: 'Air carries heat away, so the roaster behaves differently at 100 % and 75 % fan. Build rows from autotunes at the fan speeds you roast at; between rows the gains are interpolated. Every fan change (yours, a replay, fan assist) re-selects the gains.'
	},
	storedGains: {
		line: 'The gains the roaster uses when no schedule is active.'
	},
	deviceContract: {
		line: 'What this roaster must do for Roastwarden, and whether it does.',
		more: "A roaster sends BT and ET, takes heater and fan %, and keeps its own safety: emergency stop, over-temperature cutoffs, sensor checks, the fan interlock and a comms watchdog. An onboard PID, feedforward, the control lease and a time-proportioned heater are optional. The server holds the PID gains: firmware 3.5.0 keeps none across a reboot, so after a boot the server sends back the gain schedule's gains for the fan (else the last stored ones) when the roaster comes up with others."
	},
	roasterCheck: {
		line: 'Whether this roaster meets the interface standard: readings at a usable rate, plausible values, the units right.',
		more: 'Read from the last 30 s of samples: the rate (0.5 to 4 a second), BT and ET present and plausible, °F sent as °C (an idle roaster reading 50 to 115), each declared output echoed in telemetry, each declared channel present, what it says it can do, and the control level. It only reads. The write check is the owner’s, on a cold roaster: heater 10 %, its echo, then emergency stop, always.'
	},
	mapping: {
		line: 'How Roastwarden reads this device’s own messages and writes its commands.',
		more: 'Each row takes one value from the device’s message by a path (data.BT, probes[0].value, $ for a plain number) and gives it a role (BT, ET, heater…) or a name of its own (an extra channel, kept live). Scale and offset turn raw numbers into °C or %; with °F the temperatures are converted. Templates write a command in the device’s syntax: {value} is the value. The preview under it shows the newest message and what the mapping makes of it, before anything is saved.'
	},
	simulatedDrum: {
		line: 'A simulated gas drum roaster that speaks a stranger’s MQTT, set up like any roaster.',
		more: 'It publishes sim/drum/state in °F with its own field names and takes commands on sim/drum/cmnd/…, so Roastwarden reads it through an ingest mapping and command templates, as it would a real one. Switched on the first time, it sets up roaster virtual_drum: its connection, kind gas drum, full control, burner, air and drum outputs. It has no PID: Profile drive runs the server’s loop. Charge loads cold beans; Dump empties the drum.'
	},
	roasterModel: {
		line: "This roaster's thermal model, fitted from its own roasts: what the scope's BT model draws.",
		more: "BT rises with the heater's share over the fan (through a short lag) and falls by a curve in BT that the roasts teach it: the losses, drying, the reactions near first crack. Each roast is predicted two minutes ahead by a fit made without it, so the numbers are what to expect on a new roast. It refits by itself after every roast. Where BT settles shows the headroom: the most this roaster can reach at that heater and fan."
	},
	scopeModel: {
		line: "BT for the next two minutes from this roaster's own model, with a 90 % band.",
		more: "Under the plan: the drive's setpoints with the roaster's PID following (and where the heater runs out), the PID's setpoint held, or the heater and fan held in manual. First crack comes from the BT it came at on this bean and profile; DROP from the profile. The dashed BT → line beside it is the old projection (the RoR now). Each roast keeps both and scores them; the roaster's page shows which does better."
	},
	autotune: {
		line: 'A relay test that measures the roaster and suggests PID gains.',
		more: 'It heats to the test temperature at full power, then switches the heater around the power that holds it and measures the oscillation. Run it with an empty chamber or with beans at the fan speed you roast at; not during a roast. Results can be applied, added to the gain schedule, or undone later.'
	},
	simulator: {
		line: 'A simulated roaster in the server, modelled on yours.',
		more: 'It obeys heater, fan, setpoint, PID, feedforward and the control lease like the real firmware, so replay, alarms and fan assist behave as they would on the roaster. It runs in real time. Charge loads cold beans (BT drops to room temperature); Drop dumps them.'
	},
	timeZone: {
		line: 'The day a roast falls on when the asker gives no time zone of its own.',
		more: "Dates on labels, share pages, roasted coffee and reports are the day in your time zone. This dashboard always sends your browser's own; the roast CLI on a machine in UTC, agents and scripts do not, and get this zone instead (with daylight saving right for each date). Unset, they get UTC, so a roast late in the evening west of Greenwich would print as the next day."
	},
	publicUrl: {
		line: 'The address the links this server makes start with.',
		more: "Share pages (/R/ for a roast, /P/ for a profile), label QR codes, approval links in notifications and the roaster's page in Home Assistant all start with it. Unset, a link starts with the address its request came to: right from a browser, but from the roast CLI on the server itself or from a script it can be http://localhost:8080, which no one else can open."
	},
	lookaheadAdapt: {
		line: 'The lead follows the loop: most while drying, little after first crack.',
		more: 'The loop lags most while the beans first heat and hardly at all after first crack, so a fixed lead puts BT that far ahead of the profile late in the roast, and first crack comes early. Adapting, the lead starts at the lookahead and, from a minute after CHARGE, follows how far BT runs ahead of or behind the profile itself, never above the lookahead or below 0. The Profile drive tab shows the lead in use.'
	},
	autoDrop: {
		line: "Profile drive marks DROP by itself at this profile's drop, after a countdown you can cancel.",
		more: "For a profile that is dialed in. At the drop time it goes by profile time (an align or a resumed drive moves it with the profile); at the end temperature it waits for first crack, or the profile past its first crack, so an early spike cannot drop the roast; by development it drops the profile's drop minus first crack after you mark first crack. The Roast page counts down (Settings → Roasting, default 10 s) with Cancel, phones get a notice, and the countdown ends on the drop itself. DROP then runs the roaster's DROP step: heater off, fan to cool. First crack stays your mark. It needs the roaster's DROP step on."
	},
	rorWindow: {
		line: 'How many seconds of BT each RoR value is computed from.',
		more: 'A longer window hides probe noise but answers later: a change in BT shows up about half a window late with the moving average and Savitzky-Golay, about a third of a window with the weighted fit. Go longer if the RoR line looks like a saw, shorter if it answers too late to a heater change.'
	},
	rorMovingAverage: {
		line: 'The BT change from the start of the window to its end. Simple and steady; about half a window late.',
		more: 'The slope between the first and the last reading in the window. Readings in between are ignored, so one noisy reading at either end moves it.'
	},
	rorWeighted: {
		line: 'A best-fit slope with recent readings counting more. Reacts sooner, a little noisier.',
		more: 'A least-squares line through every reading in the window, weighted from oldest (least) to newest (most). It uses all the readings, so single noisy ones matter less than with the moving average, and it follows a change sooner at the same window length. Good when you steer by RoR during the roast.'
	},
	rorSavitzkyGolay: {
		line: 'A curve fitted through the window, its slope at the middle. Smoothest; keeps bends like the dip at first crack.',
		more: 'A second-order (curved) least-squares fit through every reading in the window, read at the window’s middle (Savitzky-Golay). It removes the most noise without flattening bends in the RoR, but it describes half a window ago. Good for reading the shape of a roast; the weighted fit answers faster live.'
	},
	cupQuality: {
		line: 'Quality on the form’s scale: 6 good, 7 very good, 8 excellent, 9 outstanding, in quarter points.',
		more: 'The classic SCA cupping form (2004 protocol). Seven attributes score quality from 6.00 to 9.75 in quarter points; in theory the scale runs 0–10. The ten attributes add up to the total.'
	},
	cupCups: {
		line: 'Five cups, 2 points each: untick a cup that differs (uniformity), has an off taste (clean cup) or lacks sweetness.',
		more: 'Uniformity, clean cup and sweetness are judged cup by cup over the sample’s five cups. A cup that passes earns 2 points, so every cup passing is 10, the usual score.'
	},
	cupDefects: {
		line: 'Off flavours: a taint takes 2 points a cup, a fault 4, off the total.',
		more: 'A taint is noticeable but not overwhelming, mostly in the aroma; a fault spoils the taste. Count the cups with each (five at most together). Final score = total − defects; 80 and over is specialty grade (80 very good, 85 excellent, 90 outstanding). These are classic scores, never converted to the 2024 CVA form.'
	}
} satisfies Record<string, Help>;
