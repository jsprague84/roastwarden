<script lang="ts">
	/**
	 * Modules (docs/lean-home.md): the optional parts of the dashboard, a preset or one switch
	 * each. Saved at once; an off module leaves the menus, its data and API stay.
	 */
	import { MODULES, MODULE_INFO, PRESETS, presetOf, type Module } from '#lib/modules/modules.js';
	import { modulesState, saveModules } from '#lib/stores/modules.svelte.js';
	import { notifications } from '#lib/stores/notifications.js';
	import ModulePresets from '#lib/components/settings/ModulePresets.svelte';

	let busy = $state(false);
	const preset = $derived(presetOf(modulesState.on));

	async function save(on: Module[]) {
		busy = true;
		try {
			await saveModules(on);
		} catch (e) {
			notifications.add(
				`Could not save the modules: ${e instanceof Error ? e.message : e}`,
				'error'
			);
		} finally {
			busy = false;
		}
	}

	const toggle = (m: Module) =>
		save(
			modulesState.on.includes(m) ? modulesState.on.filter((x) => x !== m) : [...modulesState.on, m]
		);
</script>

<p class="text-sm text-muted-foreground">
	Turn off what you do not use and it leaves the menus and pages. Nothing is deleted: turn it back
	on and it is all there.
</p>

<div class="mt-4 divide-y divide-border border-y border-border">
	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Preset</h2>
		<div class="mt-2">
			<ModulePresets value={preset} disabled={busy} onpick={(p) => save(PRESETS[p])} />
		</div>
		{#if preset === 'custom'}<p class="mt-2 text-xs text-muted-foreground">
				Your own mix (no preset).
			</p>{/if}
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Modules</h2>
		<ul class="mt-2 space-y-2">
			{#each MODULES as m (m)}
				<li>
					<label class="flex items-start gap-3 text-sm">
						<input
							type="checkbox"
							class="mt-0.5"
							checked={modulesState.on.includes(m)}
							disabled={busy}
							onchange={() => toggle(m)}
						/>
						<span>
							<span class="font-medium text-foreground">{MODULE_INFO[m].name}</span>
							<span class="block text-xs text-muted-foreground">{MODULE_INFO[m].line}</span>
						</span>
					</label>
				</li>
			{/each}
		</ul>
	</section>
</div>
