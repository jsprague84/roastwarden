/**
 * Profiles by roaster kind (docs/profiles.md, "Roaster kinds"): the library lists profiles for
 * the selected roaster's kind first and marks those written for another kind ("for a drum").
 * Nothing is hidden. A profile with no kind fits any roaster; electric and gas drums count as
 * one family (a drum curve suits either), as does a logger or "other" roaster with nothing.
 */
import type { RoasterKind, RoasterSetup } from '#lib/api/roasters.js';
import { isSimulatedFluidBed } from '#lib/roast/live.js';

type Family = 'fluid_bed' | 'drum';

function family(kind: RoasterKind | null | undefined): Family | null {
	if (kind === 'fluid_bed_electric') return 'fluid_bed';
	if (kind === 'drum_electric' || kind === 'drum_gas') return 'drum';
	return null;
}

const FOR: Record<Family, string> = { fluid_bed: 'for a fluid bed', drum: 'for a drum' };

/** The selected roaster's kind, or null until its setup has loaded (the simulator is a fluid bed). */
export function selectedKind(
	setup: RoasterSetup | null,
	deviceId: string | null
): RoasterKind | null {
	if (isSimulatedFluidBed(deviceId)) return 'fluid_bed_electric';
	return setup && setup.device_id === deviceId ? setup.kind : null;
}

/** The small label on a profile written for another kind of roaster, or null. Nothing until the
 * roaster's kind is known. */
export function kindTag(
	profileKind: RoasterKind | null | undefined,
	roasterKind: RoasterKind | null
): string | null {
	if (roasterKind == null) return null;
	const f = family(profileKind);
	return f && f !== family(roasterKind) ? FOR[f] : null;
}

/** The list with profiles for another kind of roaster last; the order is kept otherwise. */
export function sortByKind<P extends { roaster_kind?: RoasterKind | null }>(
	list: P[],
	roasterKind: RoasterKind | null
): P[] {
	const other = (p: P) =>
		kindTag(p.roaster_kind, roasterKind) != null && family(roasterKind) != null ? 1 : 0;
	return list
		.map((p, i) => [p, i] as const)
		.sort(([a, i], [b, j]) => other(a) - other(b) || i - j)
		.map(([p]) => p);
}
