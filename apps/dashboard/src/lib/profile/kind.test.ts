import { describe, expect, it } from 'vitest';
import type { RoasterKind, RoasterSetup } from '#lib/api/roasters.js';
import { kindTag, selectedKind, sortByKind } from './kind.js';

const p = (id: string, roaster_kind: RoasterKind | null = null) => ({ id, roaster_kind });
const library = [
	p('mine'),
	p('drum-light-filter', 'drum_gas'),
	p('fluidbed-light', 'fluid_bed_electric'),
	p('drum-x', 'drum_electric'),
	p('fluidbed-omni', 'fluid_bed_electric')
];
const ids = (l: { id: string }[]) => l.map((x) => x.id);

describe('profiles by roaster kind', () => {
	it("lists a fluid bed's own and any-kind profiles first, in their order, and drums last", () => {
		expect(ids(sortByKind(library, 'fluid_bed_electric'))).toEqual([
			'mine',
			'fluidbed-light',
			'fluidbed-omni',
			'drum-light-filter',
			'drum-x'
		]);
	});

	it('counts electric and gas drums as one family', () => {
		expect(ids(sortByKind(library, 'drum_electric'))).toEqual([
			'mine',
			'drum-light-filter',
			'drum-x',
			'fluidbed-light',
			'fluidbed-omni'
		]);
		expect(kindTag('drum_gas', 'drum_electric')).toBeNull();
		expect(kindTag('fluid_bed_electric', 'drum_gas')).toBe('for a fluid bed');
		expect(kindTag('drum_electric', 'fluid_bed_electric')).toBe('for a drum');
	});

	it('hides nothing and marks nothing a profile with no kind', () => {
		for (const k of ['fluid_bed_electric', 'drum_gas', 'logger', 'other'] as RoasterKind[]) {
			expect(sortByKind(library, k)).toHaveLength(library.length);
			expect(kindTag(null, k)).toBeNull();
		}
	});

	it('keeps the order for a logger or other roaster, and marks the seeds', () => {
		expect(ids(sortByKind(library, 'logger'))).toEqual(ids(library));
		expect(kindTag('drum_gas', 'logger')).toBe('for a drum');
		expect(kindTag('fluid_bed_electric', 'other')).toBe('for a fluid bed');
	});

	it('does nothing until the roaster’s kind is known', () => {
		expect(ids(sortByKind(library, null))).toEqual(ids(library));
		expect(kindTag('drum_gas', null)).toBeNull();
	});

	it("takes the selected roaster's setup, the simulator as a fluid bed", () => {
		const setup = { device_id: 'drum1', kind: 'drum_gas' } as RoasterSetup;
		expect(selectedKind(setup, 'drum1')).toBe('drum_gas');
		expect(selectedKind(setup, 'other_roaster')).toBeNull();
		expect(selectedKind(null, 'drum1')).toBeNull();
		expect(selectedKind(null, 'virtual_roaster')).toBe('fluid_bed_electric');
	});
});
