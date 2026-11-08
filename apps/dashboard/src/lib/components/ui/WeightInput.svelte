<script lang="ts">
	/**
	 * A weight field in this browser's system (Settings → This browser): takes "250 g", "8 oz",
	 * "2 lb", "1.5kg"; a bare number is grams (Metric) or ounces (US). Binds grams, null when
	 * blank or not a weight (then `invalid` is true, the field shows it and its form will not
	 * submit: the message is the field's validity, which the browser shows). A stored weight
	 * shows rounded in US and stays exactly as stored until the field is edited.
	 */
	import type { HTMLInputAttributes } from 'svelte/elements';
	import { weightState } from '#lib/stores/units.svelte.js';
	import { parseWeight, smallUnit, weightInputText } from '#lib/beans/weight.js';

	interface Props extends Omit<HTMLInputAttributes, 'value' | 'type'> {
		grams?: number | null;
		invalid?: boolean;
		/** A correction: a negative weight is allowed */
		signed?: boolean;
	}

	let {
		grams = $bindable(null),
		invalid = $bindable(false),
		signed = false,
		class: cls = '',
		placeholder,
		...rest
	}: Props = $props();

	let el: HTMLInputElement | undefined = $state();
	let text = $state('');
	let error = $state('');
	// The grams and unit the text stands for (not reactive: only a change from outside rewrites it)
	let shown: number | null | undefined = undefined;
	let shownUnit: string | undefined = undefined;

	$effect.pre(() => {
		const g = grams;
		const u = weightState.unit;
		if (g === shown && u === shownUnit) return;
		text = weightInputText(g, u);
		shown = g;
		shownUnit = u;
		error = '';
		invalid = false;
	});

	$effect(() => el?.setCustomValidity(error));

	function oninput(e: Event & { currentTarget: HTMLInputElement }) {
		text = e.currentTarget.value;
		const r = text.trim() === '' ? null : parseWeight(text, weightState.unit, { signed });
		error = typeof r === 'string' ? r : '';
		invalid = typeof r === 'string';
		grams = typeof r === 'number' ? r : null;
		shown = grams;
	}
</script>

<input
	bind:this={el}
	{...rest}
	type="text"
	inputmode="decimal"
	autocomplete="off"
	value={text}
	{oninput}
	placeholder={placeholder ?? smallUnit(weightState.unit)}
	aria-invalid={invalid || undefined}
	title={error ||
		`A number is ${smallUnit(weightState.unit) === 'oz' ? 'ounces' : 'grams'}; 250 g, 8 oz, 2 lb and 1.5 kg are read too`}
	class="{cls} {invalid ? 'border-red-400' : ''}"
/>
