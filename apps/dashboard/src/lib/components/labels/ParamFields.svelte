<script lang="ts">
	/** A design's parameters as form fields (docs/labels.md, "Designs"): one per entry of the
	 *  manifest's `params`, its kind from the default (`lib/labels/params.ts`). */
	import type { ParamSpec } from '#lib/labels/params.js';

	let { specs, values = $bindable() }: { specs: ParamSpec[]; values: Record<string, unknown> } =
		$props();

	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
	const set = (k: string, v: unknown) => (values = { ...values, [k]: v });
</script>

{#each specs as p (p.key)}
	{#if p.kind === 'bool'}
		<label class="flex items-center gap-2 text-sm text-foreground" title={p.help}>
			<input
				type="checkbox"
				checked={Boolean(values[p.key])}
				onchange={(e) => set(p.key, e.currentTarget.checked)}
			/>
			{p.key}{p.help ? `: ${p.help}` : ''}
		</label>
	{:else}
		<label class={lbl}
			>{p.key}{p.help ? ` · ${p.help}` : ''}
			{#if p.kind === 'choice'}
				<select
					value={String(values[p.key] ?? '')}
					onchange={(e) => set(p.key, e.currentTarget.value)}
					class={field}
				>
					{#each p.choices ?? [] as c (c)}<option value={c}>{c}</option>{/each}
				</select>
			{:else if p.kind === 'lines'}
				<textarea
					rows="5"
					value={String(values[p.key] ?? '')}
					oninput={(e) => set(p.key, e.currentTarget.value)}
					class="{field} font-mono text-xs"></textarea>
			{:else if p.kind === 'color'}
				<span class="mt-1 flex items-center gap-2">
					<input
						type="color"
						value={String(values[p.key] ?? '#000000')}
						oninput={(e) => set(p.key, e.currentTarget.value)}
						class="h-8 w-12 shrink-0 rounded border border-border bg-input"
					/>
					<span class="font-mono text-xs text-foreground">{String(values[p.key] ?? '')}</span>
				</span>
			{:else}
				<input
					type={p.kind === 'number' ? 'number' : 'text'}
					value={String(values[p.key] ?? '')}
					oninput={(e) => set(p.key, e.currentTarget.value)}
					class={field}
				/>
			{/if}
		</label>
	{/if}
{/each}
