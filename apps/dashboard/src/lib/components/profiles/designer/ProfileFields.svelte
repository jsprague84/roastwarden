<script lang="ts">
	/**
	 * The profile's details (docs/profiles.md): its name and what it was made for, the roaster
	 * and the kind it is for, roast guidance, the charge and end temperatures, auto DROP.
	 */
	import ProfileGuidanceFields from '#lib/components/profiles/ProfileGuidanceFields.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import { HELP } from '#lib/constants/help.js';
	import { parseTemp, showTemp, type DesignerFields } from '#lib/profile/designer.js';
	import { KIND_LABEL, KINDS } from '#lib/roaster/setup.js';
	import { roundForInput, tempUnit, toDisplay, weightUnit } from '#lib/stores/units.svelte.js';

	let {
		fields = $bindable(),
		batchBad = $bindable()
	}: {
		fields: DesignerFields;
		/** The batch size typed is not a weight. */
		batchBad: boolean;
	} = $props();
</script>

<div class="rounded-lg border border-border bg-card p-4">
	<h2 class="mb-3 text-sm font-semibold text-foreground">Profile details</h2>
	<div class="grid gap-3 sm:grid-cols-2">
		<div class="sm:col-span-2">
			<label for="profile-name" class="mb-1 block text-xs font-medium text-muted-foreground"
				>Name *</label
			>
			<input
				id="profile-name"
				type="text"
				bind:value={fields.name}
				placeholder="My Roast Profile"
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
			/>
		</div>
		<div class="sm:col-span-2">
			<label for="profile-description" class="mb-1 block text-xs font-medium text-muted-foreground"
				>Description</label
			>
			<input
				id="profile-description"
				type="text"
				bind:value={fields.description}
				placeholder="Optional description"
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
			/>
		</div>
		<div class="sm:col-span-2">
			<label for="profile-beans" class="mb-1 block text-xs font-medium text-muted-foreground"
				>Beans</label
			>
			<input
				id="profile-beans"
				type="text"
				bind:value={fields.beans}
				placeholder="What it was made for, e.g. Ethiopia Guji washed"
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
			/>
		</div>
		<div>
			<label for="profile-batch" class="mb-1 block text-xs font-medium text-muted-foreground"
				>Batch size ({weightUnit()} green)</label
			>
			<WeightInput
				id="profile-batch"
				bind:grams={fields.batchSize}
				bind:invalid={batchBad}
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
			/>
		</div>
		<div>
			<label for="profile-roaster" class="mb-1 block text-xs font-medium text-muted-foreground"
				>Roaster</label
			>
			<input
				id="profile-roaster"
				type="text"
				bind:value={fields.roaster}
				placeholder="Machine"
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
			/>
		</div>
		<div>
			<label for="profile-kind" class="mb-1 block text-xs font-medium text-muted-foreground"
				>For a kind of roaster</label
			>
			<select
				id="profile-kind"
				bind:value={fields.roasterKind}
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
			>
				<option value="">Any</option>
				{#each KINDS as k (k)}<option value={k}>{KIND_LABEL[k]}</option>{/each}
			</select>
		</div>
		<ProfileGuidanceFields
			bind:level={fields.roastLevel}
			bind:purpose={fields.roastPurpose}
			bind:expectations={fields.expectations}
		/>
		<div>
			<label for="charge-temp" class="mb-1 block text-xs font-medium text-muted-foreground"
				>Charge Temp ({tempUnit()})</label
			>
			<input
				id="charge-temp"
				type="number"
				value={showTemp(fields.chargeTemp)}
				oninput={(e) => {
					const v = (e.target as HTMLInputElement).value;
					fields.chargeTemp = v === '' ? undefined : parseTemp(parseFloat(v));
				}}
				placeholder={String(roundForInput(toDisplay(200), 0))}
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
			/>
		</div>
		<div>
			<label for="target-end-temp" class="mb-1 block text-xs font-medium text-muted-foreground"
				>Target End Temp ({tempUnit()})</label
			>
			<input
				id="target-end-temp"
				type="number"
				value={showTemp(fields.targetEndTemp)}
				oninput={(e) => {
					const v = (e.target as HTMLInputElement).value;
					fields.targetEndTemp = v === '' ? undefined : parseTemp(parseFloat(v));
				}}
				placeholder={String(roundForInput(toDisplay(215), 0))}
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground"
			/>
		</div>
		<div>
			<label
				for="auto-drop"
				class="mb-1 flex items-center gap-1 text-xs font-medium text-muted-foreground"
			>
				Auto DROP <Hint inline help={HELP.autoDrop} />
			</label>
			<select
				id="auto-drop"
				bind:value={fields.autoDrop}
				class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
			>
				<option value="">Off: drop by hand</option>
				<option value="time">At the drop time (the curve's end)</option>
				<option value="bt">At the end temperature</option>
				<option value="development">Development after first crack</option>
			</select>
		</div>
	</div>
</div>
