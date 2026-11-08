import { describe, expect, it } from 'vitest';
import type { PlannedRoast } from '#lib/api/plans.js';
import {
	batchLabel,
	fmtDelta,
	fmtGrams,
	moveId,
	nextPlan,
	planLabel,
	queueStock,
	stockNote
} from './stock.js';

function plan(id: string, over: Partial<PlannedRoast> = {}): PlannedRoast {
	return {
		id,
		position: 1,
		device_id: null,
		bean_id: null,
		profile_id: null,
		green_weight_g: null,
		name: null,
		notes: null,
		session_id: null,
		created_at: '',
		updated_at: '',
		bean_name: null,
		bean_stock_g: null,
		profile_name: null,
		batch_number: null,
		...over
	};
}

describe('beans and plans', () => {
	it('formats grams', () => {
		expect(fmtGrams(850)).toBe('850 g');
		expect(fmtGrams(1250)).toBe('1.25 kg');
		expect(fmtGrams(-150)).toBe('−150 g');
		expect(fmtGrams(null)).toBe('–');
		expect(fmtDelta(2000)).toBe('+2 kg');
		expect(fmtDelta(-100)).toBe('−100 g');
	});

	it('says what a roast leaves', () => {
		const bean = { stock_g: 900, low_stock_g: 500 };
		expect(stockNote(bean, 100)).toEqual({
			level: 'ok',
			text: '900 g in stock, 800 g after this roast'
		});
		expect(stockNote(bean, 450).level).toBe('low');
		expect(stockNote(bean, 950)).toEqual({
			level: 'short',
			text: 'Not enough: 900 g in stock for 950 g'
		});
		expect(stockNote({ stock_g: 0, low_stock_g: 500 }, 100).level).toBe('out');
		expect(stockNote(bean, null)).toEqual({ level: 'ok', text: '900 g in stock' });
	});

	it('labels a plan', () => {
		expect(
			planLabel(
				plan('a', { bean_name: 'Brazil', green_weight_g: 100, profile_name: 'Light espresso' })
			)
		).toBe('Brazil · 100 g · Light espresso');
		expect(planLabel(plan('a', { name: 'Filter', bean_name: 'Kenya' }))).toBe('Filter: Kenya');
		expect(planLabel(plan('a'))).toBe('Unspecified roast');
		// The Roast page shows the profile in its own picker.
		expect(
			planLabel(
				plan('a', { bean_name: 'Brazil', green_weight_g: 100, profile_name: 'Light espresso' }),
				{ profile: false }
			)
		).toBe('Brazil · 100 g');
	});

	it('walks the queue per bean', () => {
		const q = queueStock([
			plan('1', { bean_id: 'k', bean_stock_g: 250, green_weight_g: 100 }),
			plan('2', { bean_id: 'b', bean_stock_g: 1000, green_weight_g: 100 }),
			plan('3', { bean_id: 'k', bean_stock_g: 250, green_weight_g: 100 }),
			plan('4', { bean_id: 'k', bean_stock_g: 250, green_weight_g: 100 }),
			plan('done', { bean_id: 'k', bean_stock_g: 250, green_weight_g: 100, session_id: 's' })
		]);
		expect(q.get('1')).toEqual({ leftG: 150, short: false });
		expect(q.get('3')).toEqual({ leftG: 50, short: false });
		expect(q.get('4')).toEqual({ leftG: -50, short: true });
		expect(q.get('2')?.short).toBe(false);
		expect(q.has('done')).toBe(false);
	});

	it('offers the next plan, skipping', () => {
		const plans = [plan('done', { session_id: 's' }), plan('a'), plan('b')];
		expect(nextPlan(plans, new Set())).toMatchObject({ index: 0, count: 2, plan: { id: 'a' } });
		expect(nextPlan(plans, new Set(['a']))).toMatchObject({ index: 1, plan: { id: 'b' } });
		expect(nextPlan(plans, new Set(['a', 'b']))).toBeNull();
	});

	it('numbers and reorders', () => {
		expect(batchLabel(12)).toBe('#12');
		expect(batchLabel(null)).toBe('');
		expect(moveId(['a', 'b', 'c'], 'c', -1)).toEqual(['a', 'c', 'b']);
		expect(moveId(['a', 'b', 'c'], 'a', -1)).toEqual(['a', 'b', 'c']);
	});
});
