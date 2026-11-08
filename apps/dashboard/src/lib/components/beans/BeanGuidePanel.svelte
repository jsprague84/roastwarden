<script lang="ts">
	/**
	 * A bean's guide (docs/new-bean.md): before its first roast, what to expect; after a
	 * roast, what to change for the next one. The server writes the lines; this lists them
	 * and hides itself once the bean has three roasts on its profile (its history reads them
	 * then). The one profile change a line may suggest opens the profile's own editor:
	 * nothing changes here.
	 */
	import { beans as beansApi, type BeanGuide } from '#lib/api/beans.js';
	import { profiles as profilesApi, type RoastProfile } from '#lib/api/profiles.js';
	import { resolve } from '$app/paths';
	import { guideHeading } from '#lib/beans/guide.js';
	import Hint from '#lib/components/ui/Hint.svelte';

	let {
		beanId,
		after = null,
		pickProfile = false,
		heading = 'h2',
		class: cls = ''
	}: {
		beanId: string;
		/** A roast of the bean: the next roast after it (History) */
		after?: string | null;
		/** Before the first roast, offer the profile to read it on (the bean page) */
		pickProfile?: boolean;
		heading?: 'h2' | 'h3';
		class?: string;
	} = $props();

	let guide = $state<BeanGuide | null>(null);
	let profileList = $state<RoastProfile[]>([]);
	let profile = $state<string | null>(null);

	async function load() {
		try {
			guide = await beansApi.guide(beanId, { after, profile });
		} catch {
			guide = null;
		}
	}
	$effect(() => {
		void beanId;
		void after;
		void profile;
		load();
	});
	$effect(() => {
		if (!pickProfile) return;
		profilesApi
			.list()
			.then((l) => (profileList = l))
			.catch(() => (profileList = []));
	});

	const title = $derived(guideHeading(guide));
</script>

{#if guide && title}
	<section aria-label={title} class="space-y-2 {cls}">
		<div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
			<svelte:element
				this={heading}
				class={heading === 'h2'
					? 'text-lg font-semibold text-foreground'
					: 'text-sm font-semibold text-foreground'}>{title}</svelte:element
			>
			<span class="text-xs text-muted-foreground">
				{#if guide.stage === 'next' && guide.after}after {guide.after.batch_number != null
						? `#${guide.after.batch_number}`
						: guide.after.name}{/if}
				{#if guide.profile}{guide.stage === 'next' ? ' · ' : 'on '}{guide.profile}{/if}
				{#if guide.roaster}
					· {guide.roaster}{/if}
			</span>
			{#if pickProfile && guide.stage === 'before'}
				<select
					class="rounded-md border border-border bg-input px-2 py-0.5 text-xs text-foreground focus:border-focus focus:outline-none"
					aria-label="Profile to read the first roast on"
					value={profile ?? guide.profile_id ?? ''}
					onchange={(e) => (profile = e.currentTarget.value || null)}
				>
					<option value="">{guide.profile_id ? 'The plan’s profile' : 'No profile'}</option>
					{#each profileList as p (p.id)}
						<option value={p.id}>{p.name}</option>
					{/each}
				</select>
			{/if}
		</div>
		<ul class="space-y-1 text-sm">
			{#each guide.lines as l (l.key + l.text)}
				<li
					class="flex gap-2 {l.kind === 'hint' ? 'text-muted-foreground' : 'text-foreground'}"
					title={l.source}
				>
					<span
						class="mt-[8px] h-1 w-1 shrink-0 rounded-full bg-muted-foreground"
						aria-hidden="true"
					></span>
					<span class="min-w-0">
						{l.text}
						{#if l.action === 'auto_drop_development' && guide.profile_id}
							<a
								href={resolve('/profiles/[id]', { id: encodeURIComponent(guide.profile_id) })}
								class="ml-1 rounded-md border border-border px-2 py-0.5 text-xs font-medium text-foreground hover:bg-accent"
								title="Opens {guide.profile ??
									'the profile'}: set its Auto DROP to development. It applies to every roast on that profile."
							>
								Drop by development…
							</a>
						{/if}
					</span>
				</li>
			{/each}
		</ul>
		{#if guide.tasting}
			<Hint help={{ line: 'If it tastes…', more: guide.tasting }} />
		{/if}
	</section>
{/if}
