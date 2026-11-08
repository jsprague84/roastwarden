<script lang="ts">
	/**
	 * A serial TC4 connection's fields, for the Add roaster wizard and a roaster's Add
	 * connection form: the port, the CHAN mapping, the reply format and the fan's output
	 * (docs/hardware-integration-guide.md, "Serial Connection").
	 */
	import Hint from '#lib/components/ui/Hint.svelte';
	import {
		FAN_HELP,
		FAN_LABEL,
		FAN_OUTPUTS,
		REPLY_FORMATS,
		REPLY_HELP,
		REPLY_LABEL,
		chanLine,
		replyShape,
		tc4Problems,
		type Tc4Settings
	} from '#lib/roaster/tc4.js';

	/** `main`: the port and the reply format; `more`: baud, poll, channels and the fan output (the
	 * connection form's Advanced fold); `all`: both. */
	let {
		settings = $bindable(),
		idPrefix = 'tc4',
		part = 'all'
	}: { settings: Tc4Settings; idPrefix?: string; part?: 'all' | 'main' | 'more' } = $props();
	const main = $derived(part !== 'more');
	const more = $derived(part !== 'main');

	const input =
		'mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus';
	const channels = [
		{ key: 'et_channel', label: 'ET channel' },
		{ key: 'bt_channel', label: 'BT channel' },
		{ key: 't3_channel', label: 'T3 channel' },
		{ key: 't4_channel', label: 'T4 channel' }
	] as const;

	let problems = $derived(tc4Problems(settings));
</script>

<div class="space-y-4">
	{#if main}
		<div>
			<label for="{idPrefix}-port" class="block text-sm font-medium text-foreground">
				Serial Port <span class="text-red-400">*</span>
			</label>
			<input
				id="{idPrefix}-port"
				type="text"
				bind:value={settings.port}
				placeholder="/dev/ttyUSB0"
				class={input}
			/>
			<p class="mt-1 text-xs text-muted-foreground">
				USB or UART port on the server running Roastwarden. The device must speak the TC4 / aArtisan
				line protocol (READ, OT1, IO3, PID;SV).
			</p>
		</div>
		<div>
			<span class="flex items-center gap-1 text-sm font-medium text-foreground">
				<label for="{idPrefix}-reply">Reply format</label>
				<Hint inline help={REPLY_HELP[settings.reply_format]} />
			</span>
			<select id="{idPrefix}-reply" bind:value={settings.reply_format} class={input}>
				{#each REPLY_FORMATS as f (f)}
					<option value={f}>{REPLY_LABEL[f]}</option>
				{/each}
			</select>
		</div>
	{/if}
	{#if more}
		<div class="grid grid-cols-2 gap-4">
			<div>
				<label for="{idPrefix}-baud" class="block text-sm font-medium text-foreground">Baud</label>
				<input
					id="{idPrefix}-baud"
					type="number"
					bind:value={settings.baud}
					min={1200}
					step={1200}
					class={input}
				/>
			</div>
			<div>
				<label for="{idPrefix}-poll" class="block text-sm font-medium text-foreground"
					>Poll Interval (ms)</label
				>
				<input
					id="{idPrefix}-poll"
					type="number"
					bind:value={settings.poll_interval_ms}
					min={250}
					step={250}
					class={input}
				/>
			</div>
		</div>
		<div>
			<div class="grid grid-cols-2 gap-4 sm:grid-cols-4">
				{#each channels as c (c.key)}
					<div>
						<label for="{idPrefix}-{c.key}" class="block text-sm font-medium text-foreground"
							>{c.label}</label
						>
						<input
							id="{idPrefix}-{c.key}"
							type="number"
							bind:value={settings[c.key]}
							min={0}
							max={4}
							class={input}
						/>
					</div>
				{/each}
			</div>
			<p class="mt-1 text-xs text-muted-foreground">
				The physical input (1–4) behind each channel, 0 for off. Sends <code
					>{chanLine(settings)}</code
				>; expects
				<code>{replyShape(settings)}</code>. T3 and T4 reach the live readings as <code>t3</code>
				and <code>t4</code>.
			</p>
		</div>
		<div>
			<span class="flex items-center gap-1 text-sm font-medium text-foreground">
				<label for="{idPrefix}-fan">Fan output</label>
				<Hint inline help={FAN_HELP} />
			</span>
			<select id="{idPrefix}-fan" bind:value={settings.fan_output} class={input}>
				{#each FAN_OUTPUTS as f (f)}
					<option value={f}>{FAN_LABEL[f]}</option>
				{/each}
			</select>
			{#if settings.fan_output === 'none'}
				<p class="mt-1 text-xs text-muted-foreground">
					Also switch the fan output off in the roaster's Control → Setup → Outputs, so nothing
					tries to set it.
				</p>
			{/if}
		</div>
	{/if}
	{#if main}
		{#if problems.length > 0}
			<ul class="space-y-0.5 text-xs text-red-400">
				{#each problems as p (p)}
					<li>{p}</li>
				{/each}
			</ul>
		{/if}
	{/if}
</div>
