<script lang="ts">
	/** Two versions of a design side by side, the changes marked (@codemirror/merge). */
	import { onMount, onDestroy } from 'svelte';
	import { EditorView, lineNumbers } from '@codemirror/view';
	import { EditorState } from '@codemirror/state';
	import { MergeView } from '@codemirror/merge';
	import { typst } from '#lib/labels/typstLanguage.js';

	let { a, b, aLabel, bLabel }: { a: string; b: string; aLabel: string; bLabel: string } = $props();

	let host: HTMLDivElement;
	let view: MergeView | undefined;

	const theme = EditorView.theme({
		'&': { fontSize: '12px', backgroundColor: 'var(--input)', color: 'var(--foreground)' },
		'.cm-scroller': {
			fontFamily: '"JetBrains Mono", ui-monospace, monospace',
			fontVariantLigatures: 'none'
		},
		'.cm-gutters': {
			backgroundColor: 'var(--card)',
			color: 'var(--muted-foreground)',
			borderRight: '1px solid var(--border)'
		},
		'.cm-changedLine': { backgroundColor: 'color-mix(in oklab, var(--go) 14%, transparent)' },
		'.cm-deletedChunk': {
			backgroundColor: 'color-mix(in oklab, var(--destructive) 14%, transparent)'
		}
	});

	const extensions = [
		lineNumbers(),
		typst({ lint: false, complete: false }),
		EditorView.editable.of(false),
		EditorState.readOnly.of(true),
		EditorView.lineWrapping,
		theme
	];

	function make() {
		view?.destroy();
		view = new MergeView({
			parent: host,
			a: { doc: a, extensions },
			b: { doc: b, extensions },
			collapseUnchanged: { margin: 3, minSize: 6 },
			gutter: true
		});
	}

	onMount(make);
	onDestroy(() => view?.destroy());
	$effect(() => {
		void a;
		void b;
		if (host) make();
	});
</script>

<div class="grid grid-cols-2 text-xs text-muted-foreground">
	<span>{aLabel}</span><span class="pl-2">{bLabel}</span>
</div>
<div
	bind:this={host}
	class="mt-1 max-h-[32rem] overflow-auto rounded-md border border-border"
></div>
