/**
 * Settings → About's versions (docs/notifications.md, "About"). The version moves only at a
 * release, so a build after one says how far past it it is.
 */
import type { Build } from '#lib/api/about.js';

/** "0.4.0 + 66 commits since its release", or "0.4.0" for the release itself or a local build */
export function versionLine(version: string, build: Build | null | undefined): string {
	const n = build?.commits_since_release;
	if (n == null || n <= 0) return version;
	return `${version} + ${n} commit${n === 1 ? '' : 's'} since its release`;
}

const known = (rev: string) => /^[0-9a-f]{7,}$/i.test(rev);

/** The open dashboard was built from another commit than the server runs: reload it */
export function dashboardIsOlder(dashboardRevision: string, serverRevision: string): boolean {
	return known(dashboardRevision) && known(serverRevision) && dashboardRevision !== serverRevision;
}
