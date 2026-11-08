import { describe, expect, it } from 'vitest';
import { changedParams, paramSpecs, paramsFrom, startValues } from './params.js';

const manifest = {
	name: 'ASCII',
	params: {
		art: { default: 'cup', choices: ['cup', 'bean', 'none'], help: 'the drawing' },
		rule: '-',
		size: 12,
		flourish: true,
		ink: '#000000',
		poem: { default: 'a\nb', help: 'two lines' }
	}
};

describe('a design parameters form', () => {
	it('reads each parameter as a field of the right kind', () => {
		const specs = paramSpecs(manifest);
		expect(specs.map((s) => [s.key, s.kind])).toEqual([
			['art', 'choice'],
			['rule', 'text'],
			['size', 'number'],
			['flourish', 'bool'],
			['ink', 'color'],
			['poem', 'lines']
		]);
		expect(specs[0].choices).toEqual(['cup', 'bean', 'none']);
		expect(paramSpecs(null)).toEqual([]);
		expect(paramSpecs({ params: [1] })).toEqual([]);
	});

	it('sends only what differs from the defaults, typed', () => {
		const specs = paramSpecs(manifest);
		const v = startValues(specs, { art: 'bean' });
		expect(v.art).toBe('bean');
		expect(v.size).toBe(12);
		v.size = '14';
		v.flourish = false;
		expect(changedParams(specs, v)).toEqual({ art: 'bean', size: 14, flourish: false });
		expect(changedParams(specs, startValues(specs))).toEqual({});
	});

	it("sends a value put back to its default over the coffee's own", () => {
		const specs = paramSpecs({
			params: { art: { default: 'cup', choices: ['cup', 'beans'] }, size: 10 }
		});
		const own = { art: 'beans' };
		// Started from the coffee's own and left: the same as its own
		expect(paramsFrom(specs, startValues(specs, own), own)).toEqual({ art: 'beans' });
		// Put back to the default: sent, or the coffee's own would win
		expect(paramsFrom(specs, { art: 'cup', size: 10 }, own)).toEqual({ art: 'cup' });
		expect(paramsFrom(specs, { art: 'cup', size: 12 })).toEqual({ size: 12 });
	});
});
