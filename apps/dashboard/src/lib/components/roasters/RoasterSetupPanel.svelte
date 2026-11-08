<script lang="ts">
	/**
	 * A roaster's setup (docs/roaster-setup.md): what kind it is, what Roastwarden may send it,
	 * its outputs (label, range, step) and fan floor, and its declared channels (docs/channels.md,
	 * RoasterChannelsEditor). New roasters log only; only the owner
	 * key changes it; raising control asks once, and so does a kind change (it rewrites the
	 * outputs, the roast sequence and fan assist to the kind's defaults).
	 */
	import { Settings2 } from '@lucide/svelte';
	import { auth, isOwnerCredential } from '#lib/api/auth.js';
	import {
		roasterSetup,
		type RoasterKind,
		type RoasterOutputs,
		type RoasterSetup,
		type SetupChange
	} from '#lib/api/roasters.js';
	import { setupSaved } from '#lib/stores/roast.svelte.js';
	import { errorText } from '#lib/autotune/tuning.js';
	import {
		CONTROL_LEVELS,
		CONTROL_LABEL,
		CONTROL_HELP,
		KINDS,
		KIND_LABEL,
		KIND_HELP,
		OUTPUTS_HELP,
		FAN_FLOOR_HELP,
		isRaise,
		raiseConfirm,
		kindChangeConfirm,
		outputDefault,
		outputsLine,
		outputsProblem,
		OUTPUT_LABEL,
		OUTPUT_NAMES,
		type OutputName
	} from '#lib/roaster/setup.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import RoasterChannelsEditor from '#lib/components/roasters/RoasterChannelsEditor.svelte';
	import type { RoasterChannel } from '#lib/api/roasters.js';

	let { deviceId, setup = $bindable(null) }: { deviceId: string; setup?: RoasterSetup | null } =
		$props();

	let owner = $state(false);
	let error = $state<string | null>(null);
	let saving = $state(false);

	$effect(() => {
		const id = deviceId;
		setup = null;
		error = null;
		void Promise.all([roasterSetup.get(id), auth.status()])
			.then(([s, a]) => {
				if (id !== deviceId) return;
				setup = s;
				owner = isOwnerCredential(a.credential);
			})
			.catch((e) => (error = errorText(e)));
	});

	async function save(change: SetupChange): Promise<boolean> {
		if (!setup) return false;
		if (isRaise(setup.control, change.control) && !confirm(raiseConfirm(deviceId))) return false;
		if (change.kind !== setup.kind && !confirm(kindChangeConfirm(change.kind))) {
			// The select shows the old kind again.
			setup = { ...setup };
			return false;
		}
		saving = true;
		error = null;
		try {
			setup = await roasterSetup.set(deviceId, change);
			setupSaved(setup);
			editing = false;
			return true;
		} catch (e) {
			error = errorText(e);
			return false;
		} finally {
			saving = false;
		}
	}

	/** Channels save on their own: the kind and control stay as they are. */
	function saveChannels(channels: RoasterChannel[]): Promise<boolean> {
		if (!setup) return Promise.resolve(false);
		return save({ kind: setup.kind, control: setup.control, channels });
	}

	const locked = $derived(!setup || setup.fixed || !owner || saving);

	// ---- Outputs: edited as a draft, saved together ----
	let editing = $state(false);
	let draft = $state<RoasterOutputs>({ heater: null, fan: null, drum: null, damper: null });
	let draftFloor = $state(0);
	function edit() {
		if (!setup) return;
		// An older server may leave drum and damper out: not declared.
		const saved = structuredClone($state.snapshot(setup.outputs));
		draft = { ...saved, drum: saved.drum ?? null, damper: saved.damper ?? null };
		draftFloor = setup.fan_floor_pct;
		editing = true;
	}
	function toggle(name: OutputName, on: boolean) {
		const saved = setup?.outputs[name] ?? null;
		draft[name] = on
			? saved
				? structuredClone($state.snapshot(saved))
				: outputDefault(setup?.kind ?? 'other', name)
			: null;
		if (!draft.heater || !draft.fan) draftFloor = 0;
	}
	const problem = $derived(editing ? outputsProblem(draft, draftFloor) : null);
	const field =
		'mt-0.5 w-full rounded-md border border-border bg-input px-2 py-1 text-sm text-foreground';
</script>

<section class="rounded-lg border border-border bg-card p-4" aria-labelledby="roaster-setup-title">
	<div class="flex items-center gap-2">
		<Settings2 class="h-4 w-4 text-muted-foreground" />
		<h2 id="roaster-setup-title" class="text-sm font-semibold text-foreground">Setup</h2>
	</div>
	{#if !setup}
		<p class="mt-2 text-xs text-muted-foreground">{error ?? 'Loading…'}</p>
	{:else}
		<div class="mt-3 grid gap-4 sm:grid-cols-[minmax(0,14rem)_minmax(0,1fr)]">
			<div>
				<span class="flex items-center gap-1 text-xs font-medium text-muted-foreground">
					<label for="setup-kind">Kind</label>
					<Hint inline help={KIND_HELP} />
				</span>
				<select
					id="setup-kind"
					value={setup.kind}
					disabled={locked}
					onchange={(e) =>
						setup && save({ kind: e.currentTarget.value as RoasterKind, control: setup.control })}
					class="mt-1 w-full rounded-md border border-border bg-input px-2 py-1.5 text-sm text-foreground disabled:opacity-60"
				>
					{#each KINDS as k (k)}<option value={k}>{KIND_LABEL[k]}</option>{/each}
				</select>
			</div>
			<div>
				<span class="text-xs font-medium text-muted-foreground" id="setup-control-label"
					>Control</span
				>
				<div
					class="mt-1 flex flex-wrap gap-x-1 border-b border-border"
					role="radiogroup"
					aria-labelledby="setup-control-label"
				>
					{#each CONTROL_LEVELS as l (l)}
						<span class="flex items-center gap-1">
							<button
								type="button"
								role="radio"
								aria-checked={setup.control === l}
								disabled={locked}
								onclick={() =>
									setup && setup.control !== l && save({ kind: setup.kind, control: l })}
								class="-mb-px whitespace-nowrap border-b-2 px-2 py-1.5 text-sm font-medium transition-colors disabled:cursor-default
								{setup.control === l
									? 'border-action text-foreground'
									: 'border-transparent text-muted-foreground enabled:hover:text-foreground'}"
							>
								{CONTROL_LABEL[l]}
							</button>
							<Hint inline help={CONTROL_HELP[l]} />
						</span>
					{/each}
				</div>
				<Hint class="mt-1.5" help={CONTROL_HELP[setup.control]} />
			</div>
		</div>
		<!-- Outputs and the fan floor -->
		<div class="mt-4 border-t border-border pt-3">
			<div class="flex items-center justify-between gap-2">
				<span class="flex items-center gap-1 text-xs font-medium text-muted-foreground">
					Outputs <Hint inline help={OUTPUTS_HELP} />
				</span>
				{#if !editing && !locked}
					<button
						type="button"
						onclick={edit}
						class="text-xs font-medium text-foreground underline-offset-2 hover:underline"
						>Edit</button
					>
				{/if}
			</div>
			{#if !editing}
				<p class="mt-1 text-sm text-foreground" data-testid="setup-outputs">
					{outputsLine(setup.outputs, setup.fan_floor_pct)}
				</p>
			{:else}
				<div class="mt-2 space-y-3">
					{#each OUTPUT_NAMES as name (name)}
						{@const o = draft[name]}
						<div class="grid grid-cols-[6rem_minmax(0,1fr)] items-start gap-2">
							<label class="flex items-center gap-1.5 pt-1 text-sm text-foreground">
								<input
									type="checkbox"
									checked={o !== null}
									onchange={(e) => toggle(name, e.currentTarget.checked)}
								/>
								{OUTPUT_LABEL[name]}
							</label>
							{#if o}
								<div class="grid grid-cols-4 gap-2">
									<label class="text-[11px] text-muted-foreground"
										>Label<input class={field} bind:value={o.label} maxlength="24" /></label
									>
									<label class="text-[11px] text-muted-foreground"
										>Min %<input
											class={field}
											type="number"
											min="0"
											max="100"
											bind:value={o.min}
										/></label
									>
									<label class="text-[11px] text-muted-foreground"
										>Max %<input
											class={field}
											type="number"
											min="0"
											max="100"
											bind:value={o.max}
										/></label
									>
									<label class="text-[11px] text-muted-foreground"
										>Step %<input
											class={field}
											type="number"
											min="0"
											max="100"
											bind:value={o.step}
										/></label
									>
								</div>
							{:else}
								<p class="pt-1 text-xs text-muted-foreground">
									Not on this roaster: never sent, its control hidden.
								</p>
							{/if}
						</div>
					{/each}
					<div class="grid grid-cols-[6rem_minmax(0,8rem)_minmax(0,1fr)] items-center gap-2">
						<label for="setup-floor" class="flex items-center gap-1 text-sm text-foreground"
							>Fan floor <Hint inline help={FAN_FLOOR_HELP} /></label
						>
						<input
							id="setup-floor"
							class={field}
							type="number"
							min="0"
							max="100"
							bind:value={draftFloor}
							disabled={!draft.heater || !draft.fan}
						/>
						<span class="text-xs text-muted-foreground">% while heating; 0: none</span>
					</div>
					{#if problem}<p class="text-xs text-red-400">{problem}</p>{/if}
					<div class="flex gap-2">
						<button
							type="button"
							disabled={!!problem || saving}
							onclick={() =>
								setup &&
								save({
									kind: setup.kind,
									control: setup.control,
									outputs: $state.snapshot(draft),
									fan_floor_pct: draftFloor
								})}
							class="rounded-md bg-action px-3 py-1 text-xs font-medium text-action-foreground hover:bg-action-hover disabled:opacity-40"
							>Save outputs</button
						>
						<button
							type="button"
							onclick={() => (editing = false)}
							class="rounded-md border border-border px-3 py-1 text-xs font-medium text-foreground hover:bg-accent"
							>Cancel</button
						>
					</div>
				</div>
			{/if}
		</div>
		<RoasterChannelsEditor {setup} {locked} onsave={saveChannels} />
		{#if setup.fixed}
			<p class="mt-2 text-xs text-muted-foreground">The simulator always has full control.</p>
		{:else if !owner}
			<p class="mt-2 text-xs text-muted-foreground">Only the owner key can change this.</p>
		{:else if !setup.updated_at}
			<p class="mt-2 text-xs text-muted-foreground">
				Not set up yet: control is off until you choose.
			</p>
		{/if}
		{#if error}<p class="mt-1 text-xs text-red-400">{error}</p>{/if}
	{/if}
</section>
