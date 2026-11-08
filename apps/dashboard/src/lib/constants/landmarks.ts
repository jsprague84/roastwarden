export const landmarkColors: Record<string, string> = {
	charge: '#22c55e',
	turning_point: '#06b6d4',
	drying_end: '#a3a3a3',
	first_crack_start: '#f59e0b',
	first_crack_end: '#d97706',
	second_crack_start: '#ef4444',
	second_crack_end: '#dc2626',
	drop: '#7c3aed',
	cool_end: '#60a5fa'
};

/** Buttons in the landmark panel, in roast order. Turning point is auto-detected by the server. */
export const landmarkLabels: Record<string, string> = {
	charge: 'Charge',
	drying_end: 'Dry End',
	first_crack_start: 'FC Start',
	first_crack_end: 'FC End',
	second_crack_start: 'SC Start',
	second_crack_end: 'SC End',
	drop: 'Drop',
	cool_end: 'Cool End'
};

/** Labels for every event type, including ones without a button. */
export const eventLabels: Record<string, string> = {
	...landmarkLabels,
	turning_point: 'Turning Point',
	development_start: 'Development',
	custom: 'Note'
};
