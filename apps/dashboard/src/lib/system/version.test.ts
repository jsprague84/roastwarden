import { describe, expect, it } from 'vitest';
import { dashboardIsOlder, versionLine } from './version.js';

describe('versionLine', () => {
	it('says how far past its release a build is', () => {
		expect(versionLine('0.4.0', { release: '0.4.0', commits_since_release: 66 })).toBe(
			'0.4.0 + 66 commits since its release'
		);
		expect(versionLine('0.4.0', { release: '0.4.0', commits_since_release: 1 })).toBe(
			'0.4.0 + 1 commit since its release'
		);
	});
	it('is the bare version for the release itself or a build with no describe', () => {
		expect(versionLine('0.5.0', { release: '0.5.0', commits_since_release: 0 })).toBe('0.5.0');
		expect(versionLine('0.5.0', {})).toBe('0.5.0');
		expect(versionLine('0.5.0', undefined)).toBe('0.5.0');
	});
});

describe('dashboardIsOlder', () => {
	it('is true only when both revisions are known and differ', () => {
		expect(dashboardIsOlder('c9fd7cff3d28', 'c9fd7cff3d28')).toBe(false);
		expect(dashboardIsOlder('c9fd7cff3d28', '11b5b59aa0f1')).toBe(true);
		expect(dashboardIsOlder('dev', '11b5b59aa0f1')).toBe(false);
		expect(dashboardIsOlder('c9fd7cff3d28', 'unknown')).toBe(false);
	});
});
