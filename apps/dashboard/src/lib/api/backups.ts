/** Backups (docs/DEPLOY.md, "Backups"): the copies, one made now, one checked, a staged restore. */

import { request } from '#lib/api/http.js';

export interface BackupFile {
	name: string;
	bytes: number;
	created_at: string;
}

export interface BackupRun {
	trigger: 'nightly' | 'manual' | string;
	started_at: string;
	file_name: string | null;
	size_bytes: number | null;
	removed: number;
	error: string | null;
}

export interface BackupOverview {
	dir: string;
	/** Copies kept; 0 = nightly copies off. */
	keep: number;
	hour_utc: number;
	files: BackupFile[];
	last: BackupRun | null;
	next_run: string | null;
	running: boolean;
	restore_staged: boolean;
}

export interface BackupInspection {
	schema: string;
	migrations: number;
	roasts: number;
	profiles: number;
	beans: number;
}

export const backups = {
	overview: () => request<BackupOverview>('/api/backups'),
	now: () =>
		request<{ file: BackupFile; removed: number }>('/api/backups', { method: 'POST' }, true),
	check: (name: string) =>
		request<BackupInspection>(
			`/api/backups/${encodeURIComponent(name)}/check`,
			{ method: 'POST' },
			true
		),
	restore: (name: string) =>
		request<{ staged: string; inspection: BackupInspection }>(
			`/api/backups/${encodeURIComponent(name)}/restore`,
			{ method: 'POST' },
			true
		),
	cancelRestore: () => request<void>('/api/backups/restore', { method: 'DELETE' }, true)
};
