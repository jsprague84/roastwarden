import { describe, expect, it } from 'vitest';
import { assetName, completesHere, idFrom, problemsOf, typstFor, validId } from './studio.js';

describe('Label Studio helpers', () => {
	it('makes ids and asset names the server takes', () => {
		expect(idFrom("Sam's gift")).toBe('sams-gift');
		expect(idFrom('  Café Ünïcode!! ')).toBe('cafe-unicode');
		expect(validId('sams-gift')).toBe(true);
		expect(validId('two words')).toBe(false);
		expect(assetName('My Logo (2).SVG')).toBe('My-Logo-2.svg');
		expect(assetName('.hidden.png')).toBe('hidden.png');
		expect(assetName('Caveat-Regular.ttf')).toBe('Caveat-Regular.ttf');
	});

	it('writes the Typst line that uses an asset', () => {
		expect(typstFor({ name: 'logo.svg', kind: 'image' })).toBe('image("logo.svg", width: 0.6in)');
		expect(typstFor({ name: 'Caveat.ttf', kind: 'font', family: 'Caveat' })).toBe(
			'text(font: "Caveat")[…]'
		);
	});

	it('marks only the design’s own errors, by span when there is one', () => {
		const p = problemsOf([
			{
				file: 'design.typ',
				line: 3,
				column: 5,
				from: 40,
				to: 44,
				message: 'unknown variable: nmae',
				hints: ['did you mean name?']
			},
			{ file: 'roastwarden.typ', line: 9, message: 'in a helper' },
			{ message: 'took longer than 10 s' }
		]);
		expect(p).toEqual([
			{
				from: 40,
				to: 44,
				line: 3,
				column: 5,
				message: 'unknown variable: nmae (hint: did you mean name?)',
				severity: 'error'
			}
		]);
	});

	it('opens completion only after # or in a name, so Enter after a closed call is a new line', () => {
		expect(completesHere('#')).toBe(true);
		expect(completesHere('  #te')).toBe(true);
		expect(completesHere('#text(fi')).toBe(true);
		expect(completesHere('#text("hi")')).toBe(false);
		expect(completesHere('#text("hi"')).toBe(false);
		expect(completesHere('#text(size: 12pt')).toBe(false);
		expect(completesHere('#text(')).toBe(false);
		expect(completesHere('#let a = (1, ')).toBe(false);
		expect(completesHere('Hello ')).toBe(false);
		expect(completesHere('')).toBe(false);
	});
});
