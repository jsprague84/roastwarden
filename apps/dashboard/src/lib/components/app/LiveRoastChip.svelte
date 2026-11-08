<script lang="ts">
	/**
	 * "A roast is running" pill for the header on every page but the cockpit:
	 * time since CHARGE (or since the start) and the profile, one tap back.
	 */
	import { resolve } from '$app/paths';
	import { roast } from '#lib/stores/roast.svelte.js';
	import { profileState } from '#lib/stores/profile.svelte.js';
	import { elapsedSinceStart, sinceCharge, roastStage } from '#lib/roast/live.js';
	import { fmtClock } from '#lib/scope/scopeData.js';

	let now = $state(Date.now());
	$effect(() => {
		const t = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(t);
	});

	const s = $derived(roast.session);
	const stage = $derived(roastStage(s, roast.events));
	const charged = $derived(sinceCharge(s, roast.events, now));
	const label = $derived(
		!s
			? ''
			: s.status === 'paused'
				? 'Paused'
				: stage === 'charge'
					? 'Waiting for charge'
					: stage === 'dropped'
						? 'Dropped'
						: 'Roasting'
	);
	const clock = $derived(s ? fmtClock(charged ?? elapsedSinceStart(s, now)) : '');
</script>

{#if s}
	<a
		href={resolve('/')}
		class="flex min-w-0 items-center gap-2 rounded-full border border-amber-500/40 bg-amber-500/10 px-3 py-1 text-xs font-medium text-amber-400 hover:bg-amber-500/20"
		title="Back to the roast"
	>
		<span class="relative flex h-2 w-2 shrink-0">
			<span
				class="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-60"
			></span>
			<span class="relative inline-flex h-2 w-2 rounded-full bg-amber-500"></span>
		</span>
		<span class="whitespace-nowrap"
			>{label} <span class="font-mono tabular-nums">{clock}</span></span
		>
		{#if profileState.activeProfile}
			<span class="hidden max-w-[10rem] truncate text-amber-400/70 lg:inline"
				>· {profileState.activeProfile.name}</span
			>
		{/if}
	</a>
{/if}
