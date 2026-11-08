<script lang="ts">
	/**
	 * Approvals (docs/control.md "Approvals"): calls a token made that wait for the owner,
	 * starting a roast on a physical roaster and deletes. Only the owner key decides; the
	 * link in a notification opens this page with ?id=.
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { approvals, type Approval } from '#lib/api/approvals.js';
	import { auth, isOwnerCredential, type AuthStatus } from '#lib/api/auth.js';
	import { isOpen, outcome } from '#lib/approvals/state.js';
	import { ago } from '#lib/tokens/state.js';
	import { notifications } from '#lib/stores/notifications.js';

	let status = $state<AuthStatus | null>(null);
	let rows = $state<Approval[]>([]);
	let loaded = $state(false);
	let busy = $state<string | null>(null);
	let now = $state(Date.now());

	const owner = $derived(isOwnerCredential(status?.credential) || status?.credential === 'open');
	const focus = $derived(page.url.searchParams.get('id'));
	const pending = $derived(rows.filter(isOpen));
	const decided = $derived(rows.filter((a) => !isOpen(a)).slice(0, 30));
	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	async function load() {
		try {
			status ??= await auth.status();
			if (owner) rows = await approvals.list();
			now = Date.now();
		} catch (e) {
			notifications.add(`Could not read the approvals: ${msg(e)}`, 'error');
		} finally {
			loaded = true;
		}
	}
	$effect(() => {
		void load();
		const t = setInterval(load, 10_000);
		return () => clearInterval(t);
	});

	async function decide(a: Approval, approve: boolean) {
		busy = a.id;
		try {
			const r = approve ? await approvals.approve(a.id) : await approvals.deny(a.id);
			notifications.add(outcome(r), r.status === 'failed' ? 'error' : 'success');
			await load();
		} catch (e) {
			notifications.add(`Could not ${approve ? 'approve' : 'deny'}: ${msg(e)}`, 'error');
			await load();
		} finally {
			busy = null;
		}
	}

	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
</script>

<p class="text-sm text-muted-foreground">
	A token set to ask first does not start a roast on a physical roaster, or delete a roast, bean,
	profile or roaster, until you say yes here. The request goes out through your notification
	channels with a link to this page and waits an hour. Approving makes the call as the token; live
	control during a roast stays behind each roaster's own switch.
</p>

{#if status && !owner}
	<p class="mt-4 border-l-2 border-amber-400 pl-3 text-sm text-foreground">
		Only the owner key approves or denies; enter it under <a
			href={resolve('/settings')}
			class="underline">This browser</a
		>.
	</p>
{:else if loaded}
	<div class="mt-4 divide-y divide-border border-y border-border">
		<section class="py-5" data-testid="approvals-pending">
			<h2 class="text-sm font-semibold text-foreground">Waiting for you</h2>
			{#if pending.length}
				<ul class="mt-2 divide-y divide-border">
					{#each pending as a (a.id)}
						<li
							class="flex flex-wrap items-center gap-3 py-3 {focus === a.id
								? 'border-l-2 border-highlight pl-3'
								: ''}"
						>
							<div class="min-w-0 flex-1">
								<p class="text-sm font-medium text-foreground">{a.summary}</p>
								<p class="mt-0.5 text-xs text-muted-foreground">
									token “{a.token_name}”{a.client_ip ? ` from ${a.client_ip}` : ''} · asked {ago(
										a.created_at,
										now
									)} · {outcome(a, now).replace('Waiting · ', '')}
								</p>
							</div>
							<div class="flex shrink-0 gap-2">
								<button
									class="rounded-md bg-go px-3 py-1.5 text-sm font-medium text-go-foreground hover:bg-go-hover disabled:opacity-50"
									disabled={busy !== null}
									onclick={() => decide(a, true)}>Approve</button
								>
								<button class={secondary} disabled={busy !== null} onclick={() => decide(a, false)}
									>Deny</button
								>
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="mt-2 text-sm text-muted-foreground">Nothing is waiting.</p>
			{/if}
		</section>

		<section class="py-5">
			<h2 class="text-sm font-semibold text-foreground">Decided</h2>
			{#if decided.length}
				<ul class="mt-2 divide-y divide-border text-sm">
					{#each decided as a (a.id)}
						<li class="py-2.5 {focus === a.id ? 'border-l-2 border-highlight pl-3' : ''}">
							<p class="text-foreground">{a.summary}</p>
							<p class="mt-0.5 text-xs text-muted-foreground">
								token “{a.token_name}” ·
								<span class={a.status === 'failed' ? 'text-red-400' : ''}>{outcome(a, now)}</span>
							</p>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="mt-2 text-sm text-muted-foreground">No requests yet.</p>
			{/if}
		</section>
	</div>
{/if}
