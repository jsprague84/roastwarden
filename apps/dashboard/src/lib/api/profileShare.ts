/**
 * Shared profiles (docs/profiles.md, "Sharing a profile"): a profile's public page, and the import
 * from another server's link.
 */

import { request } from '#lib/api/http.js';
import type { ProfileWithPoints } from '#lib/api/profiles.js';

export interface ProfileShareLink {
	code: string;
	profile_id: string;
	name: string;
	created_at: string;
	created_by: string;
	revoked_at?: string | null;
	views: number;
	downloads: number;
	last_used_at?: string | null;
	/** The page's address, `https://…/P/CODE` */
	url: string;
}

/** A profile as shared (format `roastwarden.profile/1`): what the import shows first */
interface SharedProfile {
	format: string;
	name: string;
	description?: string | null;
	roast_level?: string | null;
	roast_purpose?: string | null;
	roaster_kind?: string | null;
	target_total_time?: number | null;
	target_first_crack?: number | null;
	target_end_temp?: number | null;
	batch_size_g?: number | null;
	points: { time_seconds: number; target_temp: number }[];
	events: unknown[];
	shared_by?: string | null;
}

export interface LinkImport {
	source: string;
	profile: SharedProfile;
	saved?: ProfileWithPoints | null;
}

export const profileShare = {
	/** A profile's links, newest first, the revoked ones too */
	forProfile: (profileId: string) =>
		request<ProfileShareLink[]>(`/api/profiles/${encodeURIComponent(profileId)}/share`),
	/** The profile's newest link, else a new one; `fresh` always makes another */
	make: (profileId: string, fresh = false) =>
		request<ProfileShareLink>(`/api/profiles/${encodeURIComponent(profileId)}/share`, {
			method: 'POST',
			body: JSON.stringify({ new: fresh })
		}),
	revoke: (code: string) =>
		request<ProfileShareLink>(`/api/profile-shares/${encodeURIComponent(code)}`, {
			method: 'DELETE'
		}),
	/** Read (preview) or import another server's shared profile from its link */
	importLink: (url: string, opts: { name?: string; preview?: boolean } = {}) =>
		request<LinkImport>('/api/profiles/import/link', {
			method: 'POST',
			body: JSON.stringify({ url, name: opts.name || undefined, preview: opts.preview ?? false })
		})
};
