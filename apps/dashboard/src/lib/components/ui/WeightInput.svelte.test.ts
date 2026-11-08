import { afterEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import WeightInput from '#lib/components/ui/WeightInput.svelte';
import { setWeightUnit } from '#lib/stores/units.svelte.js';

let app: Record<string, unknown> | null = null;
afterEach(() => {
	if (app) unmount(app);
	app = null;
	document.body.innerHTML = '';
	setWeightUnit('metric');
});

function field(props: { grams: number | null; invalid: boolean; signed?: boolean }) {
	app = mount(WeightInput, { target: document.body, props });
	flushSync();
	return document.querySelector('input') as HTMLInputElement;
}

function type(el: HTMLInputElement, text: string) {
	el.value = text;
	el.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

describe('a weight field', () => {
	it('shows the stored grams, and in US ounces, without changing them', () => {
		const props = $state({ grams: 100 as number | null, invalid: false });
		const el = field(props);
		expect(el.value).toBe('100');
		expect(el.placeholder).toBe('g');
		setWeightUnit('us');
		flushSync();
		expect(el.value).toBe('3.53 oz');
		expect(el.placeholder).toBe('oz');
		expect(props.grams).toBe(100);
	});

	it('reads what is typed: a bare number in the small unit, or with a unit', () => {
		setWeightUnit('us');
		const props = $state({ grams: null as number | null, invalid: false });
		const el = field(props);
		type(el, '8');
		expect(props.grams).toBe(226.8);
		type(el, '250 g');
		expect(props.grams).toBe(250);
		type(el, '');
		expect(props.grams).toBeNull();
		expect(props.invalid).toBe(false);
	});

	it('marks junk, and its form will not submit', () => {
		const props = $state({ grams: 100 as number | null, invalid: false });
		const el = field(props);
		type(el, '2 stone');
		expect(props.invalid).toBe(true);
		expect(props.grams).toBeNull();
		expect(el.getAttribute('aria-invalid')).toBe('true');
		expect(el.validationMessage).toMatch(/Unknown unit/);
		type(el, '-5');
		expect(props.invalid).toBe(true);
		type(el, '5');
		expect(props.invalid).toBe(false);
		expect(el.validationMessage).toBe('');
	});

	it('takes a negative change when signed', () => {
		const props = $state({ grams: null as number | null, invalid: false, signed: true });
		const el = field(props);
		type(el, '-150');
		expect(props.grams).toBe(-150);
	});

	it('follows a new value from outside (a form reset)', () => {
		const props = $state({ grams: 100 as number | null, invalid: false });
		const el = field(props);
		type(el, '120');
		props.grams = null;
		flushSync();
		expect(el.value).toBe('');
	});
});
