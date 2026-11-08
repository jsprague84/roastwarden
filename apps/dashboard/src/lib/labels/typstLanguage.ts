/**
 * Typst for the design editor: the native Lezer parser from `codemirror-lang-typst` (Typst
 * 0.15's syntax, error recovery, folding, indentation, completion of Typst's built-ins, and a
 * syntax linter that marks an unclosed bracket before the server's check answers), with the
 * dashboard's own colours in both themes instead of the package's light-only style.
 */
import {
	Language,
	LanguageSupport,
	HighlightStyle,
	syntaxHighlighting,
	languageDataProp
} from '@codemirror/language';
import { autocompletion, type CompletionSource } from '@codemirror/autocomplete';
import { tags as t } from '@lezer/highlight';
import { completesHere } from './studio.js';
import {
	TypstLezerParser,
	typstCompletionSource,
	typstLezerLanguageData,
	typstLezerHighlighting,
	typstLezerIndentation,
	typstLezerFolding,
	typstLezerIndentService,
	typstLezerFoldService,
	typstLezerLinter,
	typstTags
} from 'codemirror-lang-typst/lezer';

const typstColours = HighlightStyle.define([
	{ tag: t.comment, color: 'var(--muted-foreground)', fontStyle: 'italic' },
	{ tag: [t.keyword, t.null, t.atom, t.bool], color: 'var(--code-keyword)' },
	{ tag: t.string, color: 'var(--code-string)' },
	{ tag: t.number, color: 'var(--code-number)' },
	{ tag: t.function(t.variableName), color: 'var(--code-function)', fontWeight: '600' },
	{ tag: [t.escape, t.labelName, typstTags.interpolated], color: 'var(--code-property)' },
	{ tag: [t.operator, typstTags.mathDelimiter], color: 'var(--code-number)' },
	{ tag: t.heading, fontWeight: 'bold', textDecoration: 'underline' },
	{ tag: t.strong, fontWeight: 'bold' },
	{ tag: t.emphasis, fontStyle: 'italic' },
	{ tag: t.link, textDecoration: 'underline' },
	{ tag: [t.monospace, typstTags.listMarker], color: 'var(--muted-foreground)' },
	{ tag: t.invalid, color: 'var(--destructive)' }
]);

/** The package's completion, but popping up by itself only after `#` or in a name: its own
 *  source also offers a call's parameters after `(`, `,` or a closed string, and Enter
 *  then took the first one instead of a new line. Ctrl+Space asks anywhere. */
const typstCompletion: CompletionSource = (context) => {
	if (!context.explicit) {
		const line = context.state.doc.lineAt(context.pos);
		if (!completesHere(context.state.sliceDoc(line.from, context.pos))) return null;
	}
	return typstCompletionSource(context);
};

/** Typst language support: parser, colours, folding, indentation, completion, syntax lint. */
export function typst(options: { lint?: boolean; complete?: boolean } = {}): LanguageSupport {
	const parser = new TypstLezerParser(
		languageDataProp.add((type) => (type.isTop ? typstLezerLanguageData : undefined)),
		typstLezerHighlighting,
		typstLezerIndentation,
		typstLezerFolding
	);
	const support = [
		syntaxHighlighting(typstColours),
		typstLezerIndentService,
		typstLezerFoldService
	];
	if (options.lint !== false) support.push(typstLezerLinter);
	if (options.complete !== false) support.push(autocompletion({ override: [typstCompletion] }));
	return new LanguageSupport(new Language(typstLezerLanguageData, parser, [], 'typst'), support);
}
