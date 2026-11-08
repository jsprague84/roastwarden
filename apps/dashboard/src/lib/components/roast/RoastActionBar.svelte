<script lang="ts">
	/**
	 * Phone: the stage's primary button pinned to the bottom of the screen,
	 * so marking a landmark mid-roast is one thumb away (STOP stays in the
	 * header). Hidden on wide screens, where the Next step card is in view.
	 */
	import { roast, markLandmark, resumeRoast, completeRoast } from '#lib/stores/roast.svelte.js';
	import { roastStage, sinceCharge, elapsedSinceStart } from '#lib/roast/live.js';
	import { primaryAction, LANDMARK_ACTION } from '#lib/roast/readout.js';
	import { fmtClock } from '#lib/scope/scopeData.js';

	let now = $state(Date.now());
	$effect(() => {
		const t = setInterval(() => (now = Date.now()), 500);
		return () => clearInterval(t);
	});

	const s = $derived(roast.session);
	const stage = $derived(roastStage(s, roast.events));
	const action = $derived(primaryAction(s, roast.events));
	const clock = $derived(fmtClock(sinceCharge(s, roast.events, now) ?? elapsedSinceStart(s, now)));

	function go() {
		if (roast.busy) return;
		if (action.kind === 'resume') resumeRoast();
		else if (action.kind === 'mark') markLandmark(action.landmark);
		else if (action.kind === 'complete') completeRoast();
	}
	const label = $derived(
		action.kind === 'mark'
			? LANDMARK_ACTION[action.landmark]
			: action.kind === 'resume'
				? 'Resume'
				: 'Complete roast'
	);
	const color = $derived(
		action.kind === 'mark' && action.landmark === 'drop'
			? 'bg-violet-600 text-white'
			: action.kind === 'mark' && action.landmark !== 'charge'
				? 'bg-action text-action-foreground'
				: 'bg-go text-go-foreground'
	);
</script>

{#if s && stage !== 'setup'}
	<div
		class="fixed inset-x-0 bottom-0 z-20 border-t border-border bg-card/95 px-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] pt-2 backdrop-blur lg:hidden"
	>
		<div class="flex items-center gap-2">
			<div
				class="w-16 shrink-0 text-center font-mono text-lg font-semibold tabular-nums text-foreground"
			>
				{clock}
			</div>
			<button
				type="button"
				onclick={go}
				disabled={roast.busy}
				class="h-12 flex-1 rounded-lg text-base font-semibold disabled:opacity-50 {color}"
			>
				{roast.busy ? '…' : label}
			</button>
			{#if stage === 'roasting' && action.kind === 'mark' && action.landmark !== 'drop'}
				<button
					type="button"
					onclick={() => markLandmark('drop')}
					disabled={roast.busy}
					class="h-12 rounded-lg bg-violet-600 px-4 text-base font-semibold text-white disabled:opacity-50"
				>
					Drop
				</button>
			{/if}
		</div>
	</div>
{/if}
