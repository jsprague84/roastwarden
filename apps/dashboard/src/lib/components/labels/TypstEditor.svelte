<script lang="ts">
	/**
	 * The design editor: CodeMirror 6 with Typst's syntax (`lib/labels/typstLanguage.ts`:
	 * colours, folding, completion, syntax errors as you type), the server check's errors as
	 * markers on their spans, Ctrl/⌘+S to save, Ctrl/⌘+Enter to check now. Loaded only on the
	 * design page.
	 */
	import { onMount, onDestroy } from 'svelte';
	import { EditorState, Compartment, StateEffect, StateField } from '@codemirror/state';
	import {
		EditorView,
		keymap,
		lineNumbers,
		highlightActiveLine,
		highlightActiveLineGutter,
		drawSelection
	} from '@codemirror/view';
	import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
	import { bracketMatching, indentOnInput, foldGutter } from '@codemirror/language';
	import { search, searchKeymap, highlightSelectionMatches } from '@codemirror/search';
	import { linter, lintGutter, type Diagnostic } from '@codemirror/lint';
	import { closeBrackets, closeBracketsKeymap, completionKeymap } from '@codemirror/autocomplete';
	import { typst } from '#lib/labels/typstLanguage.js';

	/** A problem at a place: a UTF-16 range in the source, or a line (and column) */
	export type EditorProblem = {
		from?: number;
		to?: number;
		line?: number;
		column?: number;
		message: string;
		severity: 'error' | 'warning';
	};

	let {
		value = $bindable(''),
		problems = [],
		readonly = false,
		onsave,
		oncheck
	}: {
		value?: string;
		problems?: EditorProblem[];
		readonly?: boolean;
		onsave?: () => void;
		oncheck?: () => void;
	} = $props();

	let host: HTMLDivElement;
	let view: EditorView | undefined;
	const editable = new Compartment();

	// The server check's problems live in the editor's state, so they map through edits and
	// sit beside the syntax linter's own (setDiagnostics would replace those)
	const setProblems = StateEffect.define<Diagnostic[]>();
	const serverProblems = StateField.define<Diagnostic[]>({
		create: () => [],
		update(list, tr) {
			for (const e of tr.effects) if (e.is(setProblems)) return e.value;
			if (!tr.docChanged) return list;
			return list
				.map((d) => ({ ...d, from: tr.changes.mapPos(d.from), to: tr.changes.mapPos(d.to) }))
				.filter((d) => d.from <= tr.newDoc.length);
		}
	});
	const serverLint = linter((v) => v.state.field(serverProblems), {
		delay: 0,
		needsRefresh: (u) => u.transactions.some((tr) => tr.effects.some((e) => e.is(setProblems)))
	});

	const theme = EditorView.theme({
		'&': {
			height: '100%',
			fontSize: '13px',
			backgroundColor: 'var(--input)',
			color: 'var(--foreground)'
		},
		'.cm-scroller': {
			fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace',
			fontVariantLigatures: 'none',
			lineHeight: '1.5'
		},
		'.cm-content': { caretColor: 'var(--foreground)' },
		'.cm-gutters': {
			backgroundColor: 'var(--card)',
			color: 'var(--muted-foreground)',
			borderRight: '1px solid var(--border)'
		},
		'.cm-activeLine': { backgroundColor: 'color-mix(in oklab, var(--accent) 55%, transparent)' },
		'.cm-activeLineGutter': { backgroundColor: 'var(--accent)' },
		'&.cm-focused': { outline: 'none' },
		'.cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection': {
			backgroundColor: 'color-mix(in oklab, var(--focus) 30%, transparent) !important'
		},
		'.cm-cursor': { borderLeftColor: 'var(--foreground)' },
		'.cm-tooltip': {
			backgroundColor: 'var(--card)',
			color: 'var(--foreground)',
			border: '1px solid var(--border)'
		},
		'.cm-panels': { backgroundColor: 'var(--card)', color: 'var(--foreground)' }
	});

	onMount(() => {
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: value,
				extensions: [
					lineNumbers(),
					foldGutter(),
					lintGutter(),
					history(),
					drawSelection(),
					indentOnInput(),
					bracketMatching(),
					highlightActiveLine(),
					highlightActiveLineGutter(),
					highlightSelectionMatches(),
					search({ top: true }),
					typst(),
					serverProblems,
					serverLint,
					closeBrackets(),
					EditorState.tabSize.of(2),
					keymap.of([
						{ key: 'Mod-s', preventDefault: true, run: () => (onsave?.(), true) },
						{ key: 'Mod-Enter', preventDefault: true, run: () => (oncheck?.(), true) },
						indentWithTab,
						...closeBracketsKeymap,
						...completionKeymap,
						...defaultKeymap,
						...historyKeymap,
						...searchKeymap
					]),
					editable.of([EditorView.editable.of(!readonly), EditorState.readOnly.of(readonly)]),
					EditorView.updateListener.of((u) => {
						if (u.docChanged) value = u.state.doc.toString();
					}),
					theme,
					EditorView.lineWrapping
				]
			})
		});
	});

	onDestroy(() => view?.destroy());

	// A new value from outside (a version loaded, a rollback) replaces the text
	$effect(() => {
		const v = value;
		if (view && v !== view.state.doc.toString()) {
			view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } });
		}
	});

	$effect(() => {
		const r = readonly;
		view?.dispatch({
			effects: editable.reconfigure([EditorView.editable.of(!r), EditorState.readOnly.of(r)])
		});
	});

	// The check's errors and warnings, on their spans (else from the column to the line's end)
	$effect(() => {
		const list = problems;
		if (!view) return;
		const doc = view.state.doc;
		const diagnostics: Diagnostic[] = [];
		for (const p of list) {
			if (p.from !== undefined && p.from <= doc.length) {
				const to = Math.min(Math.max(p.to ?? p.from, p.from), doc.length);
				diagnostics.push({ from: p.from, to, severity: p.severity, message: p.message });
			} else if (p.line && p.line >= 1 && p.line <= doc.lines) {
				const line = doc.line(p.line);
				const from = Math.min(line.from + Math.max(0, (p.column ?? 1) - 1), line.to);
				diagnostics.push({
					from,
					to: Math.max(from, line.to),
					severity: p.severity,
					message: p.message
				});
			}
		}
		view.dispatch({ effects: setProblems.of(diagnostics) });
	});

	/** Put the cursor on a line and show it */
	export function goto(line: number) {
		if (!view) return;
		const l = view.state.doc.line(Math.min(Math.max(1, line), view.state.doc.lines));
		view.dispatch({ selection: { anchor: l.from }, scrollIntoView: true });
		view.focus();
	}
</script>

<div bind:this={host} class="h-full min-h-0 overflow-hidden rounded-md border border-border"></div>
