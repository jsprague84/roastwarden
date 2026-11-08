<script lang="ts">
	/**
	 * A Modbus register map as rows (docs/ingest-mapping.md, "Modbus TCP and RTU"): a preset, each
	 * register's address, slave (its own controller on the bus, or the connection's), name, kind,
	 * type, scale, unit, RW and role, with the role check the server
	 * makes. The Add wizard and the roaster's own page use it; the caller saves.
	 */
	import { MODBUS_PRESETS } from '#lib/constants/modbus-presets.js';
	import { bitKind, readOnlyKind } from '#lib/roaster/connection.js';
	import {
		blankRegister,
		draftOf,
		normalRole,
		ROLE_LABEL,
		roleProblem,
		rolesFor,
		slaveOf,
		type RegisterDraft,
		type RegisterRole
	} from '#lib/roaster/registerRoles.js';

	let {
		registers = $bindable(),
		presetId = $bindable('')
	}: {
		registers: RegisterDraft[];
		/** The preset the rows came from; '' offers one to replace them with */
		presetId?: string;
	} = $props();

	let problem = $derived(roleProblem(registers));

	function applyPreset(id: string) {
		const preset = MODBUS_PRESETS.find((p) => p.id === id);
		if (!preset) return;
		presetId = id;
		registers = preset.registers.map(draftOf);
	}

	// A register's kind decides what it can be: input and discrete are read-only, coils and
	// discrete inputs are one bit (bool). The server refuses a map that breaks either.
	function setKind(reg: RegisterDraft, kind: string) {
		reg.register_type = kind;
		if (readOnlyKind(kind)) reg.writable = false;
		if (bitKind(kind)) reg.data_type = 'bool';
	}

	const field =
		'rounded border border-border bg-input px-2 py-1 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-focus';
</script>

<div class="space-y-4">
	<div>
		<label for="modbus-preset" class="block text-sm font-medium text-foreground">Preset</label>
		<select
			id="modbus-preset"
			value={presetId}
			onchange={(e) => applyPreset(e.currentTarget.value)}
			class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-focus"
		>
			{#if presetId === ''}<option value="">Replace these registers with a preset…</option>{/if}
			{#each MODBUS_PRESETS as preset (preset.id)}
				<option value={preset.id}>{preset.name} &mdash; {preset.description}</option>
			{/each}
		</select>
	</div>

	{#if registers.length > 0}
		<div class="overflow-x-auto rounded-md border border-border">
			<table class="w-full text-left text-sm">
				<thead class="border-b border-border bg-muted/50">
					<tr>
						<th class="px-2 py-2 font-medium text-muted-foreground">Address</th>
						<th
							class="px-2 py-2 font-medium text-muted-foreground"
							title="The Modbus slave this register is on, when the bus has several controllers (BT on 1, ET on 2). Empty: the connection's unit id."
							>Slave</th
						>
						<th class="px-2 py-2 font-medium text-muted-foreground">Name</th>
						<th class="px-2 py-2 font-medium text-muted-foreground">Type</th>
						<th class="px-2 py-2 font-medium text-muted-foreground">Data Type</th>
						<th
							class="px-2 py-2 font-medium text-muted-foreground"
							title="The value is the raw number times this: 0.1 for a temperature in tenths (2004 is 200.4)."
							>Scale</th
						>
						<th class="px-2 py-2 font-medium text-muted-foreground">Unit</th>
						<th class="px-2 py-2 font-medium text-muted-foreground">RW</th>
						<th
							class="px-2 py-2 font-medium text-muted-foreground"
							title="What the register is, whatever its name: read as that value, or the register a command writes. —: by its name."
							>Role</th
						>
						<th class="px-2 py-2 font-medium text-muted-foreground"></th>
					</tr>
				</thead>
				<tbody class="divide-y divide-border">
					{#each registers as reg, i (i)}
						{@const offered = rolesFor(reg)}
						{@const current = normalRole(reg.role)}
						<tr class="text-foreground">
							<td class="px-2 py-1.5">
								<input
									type="number"
									bind:value={reg.address}
									aria-label="Address of row {i + 1}"
									class="w-16 {field}"
								/>
							</td>
							<td class="px-2 py-1.5">
								<input
									type="number"
									min="1"
									max="247"
									value={reg.unit_id ?? ''}
									oninput={(e) =>
										(reg.unit_id =
											slaveOf(e.currentTarget.value) ??
											(e.currentTarget.value.trim() === '' ? null : Number(e.currentTarget.value)))}
									placeholder="—"
									aria-label="Slave of row {i + 1}"
									class="w-14 {field}"
								/>
							</td>
							<td class="px-2 py-1.5">
								<input
									type="text"
									bind:value={reg.name}
									aria-label="Name of row {i + 1}"
									class="w-24 {field}"
								/>
							</td>
							<td class="px-2 py-1.5">
								<select
									value={reg.register_type}
									onchange={(e) => setKind(reg, e.currentTarget.value)}
									aria-label="Kind of row {i + 1}"
									class={field}
								>
									<option value="input">input</option>
									<option value="holding">holding</option>
									<option value="coil">coil</option>
									<option value="discrete">discrete</option>
								</select>
							</td>
							<td class="px-2 py-1.5">
								<select
									bind:value={reg.data_type}
									aria-label="Data type of row {i + 1}"
									class={field}
								>
									{#if bitKind(reg.register_type)}
										<option value="bool">bool</option>
									{:else}
										<option value="uint16">uint16</option>
										<option value="int16">int16</option>
										<option value="uint32">uint32</option>
										<option value="int32">int32</option>
										<option value="bcd16" title="Four BCD digits in one register">bcd16</option>
										<option
											value="bcd32"
											title="Eight BCD digits in two registers, in the map's word order"
											>bcd32</option
										>
										<option value="float32">float32</option>
										<option value="bool">bool</option>
									{/if}
								</select>
							</td>
							<td class="px-2 py-1.5">
								<input
									type="number"
									step="any"
									value={reg.scale_factor ?? 1}
									oninput={(e) => {
										const v = Number(e.currentTarget.value);
										if (e.currentTarget.value.trim() !== '' && Number.isFinite(v) && v !== 0)
											reg.scale_factor = v;
									}}
									aria-label="Scale of row {i + 1}"
									class="w-14 {field}"
								/>
							</td>
							<td class="px-2 py-1.5">
								<input
									type="text"
									bind:value={reg.unit}
									aria-label="Unit of row {i + 1}"
									class="w-14 {field}"
								/>
							</td>
							<td class="px-2 py-1.5 text-xs">
								<label
									class="inline-flex items-center gap-1"
									title={readOnlyKind(reg.register_type)
										? 'Input registers and discrete inputs are read-only'
										: 'Commands may write this register'}
								>
									<input
										type="checkbox"
										bind:checked={reg.writable}
										disabled={readOnlyKind(reg.register_type)}
									/>
									{reg.writable ? 'RW' : 'R'}
								</label>
							</td>
							<td class="px-2 py-1.5">
								<select
									value={current ?? ''}
									onchange={(e) => (reg.role = e.currentTarget.value || null)}
									aria-label="Role of {reg.name || `row ${i + 1}`}"
									class={field}
								>
									<option value="">—</option>
									{#each offered as role (role)}<option value={role}>{ROLE_LABEL[role]}</option
										>{/each}
									{#if current && !(offered as string[]).includes(current)}
										<!-- Kept so the problem below can name it; not offered on this row -->
										<option value={current}>{ROLE_LABEL[current as RegisterRole] ?? current}</option
										>
									{/if}
								</select>
							</td>
							<td class="px-2 py-1.5">
								<button
									type="button"
									onclick={() => (registers = registers.filter((_, j) => j !== i))}
									class="text-xs text-red-400 hover:text-red-300"
								>
									Remove
								</button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		{#if problem}<p class="text-xs text-red-400">{problem}</p>{/if}
	{:else}
		<p class="text-sm text-muted-foreground">
			No registers defined. Add registers or choose a preset.
		</p>
	{/if}

	<button
		type="button"
		onclick={() => (registers = [...registers, blankRegister()])}
		class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
	>
		+ Add Register
	</button>
</div>
