/**
 * About the server (Settings → About): version and build, modules, the built-in broker, the
 * Artisan screen, what a restart would cut.
 */

import type { BackupRun } from '#lib/api/backups.js';
import { downloadFile, request } from '#lib/api/http.js';
import type { SystemProblem } from '#lib/api/notifications.js';

export interface AboutInfo {
	name: string;
	version: string;
	revision: string;
	/** Where this build stands against the releases (`git describe` at build time) */
	build: Build;
	started_at: string;
	uptime_seconds: number;
	mqtt_ready: boolean;
	db_ready: boolean;
	migration: string | null;
	last_backup: BackupRun | null;
	problems: SystemProblem[];
	/** Null when the server does not know the repository it was built from. */
	changelog_url: string | null;
	/** The source of the running revision (AGPL-3.0 section 13); null like changelog_url */
	source_url: string | null;
	/** Roastwarden's licence, an SPDX id; its text and the notices are under /licenses/ */
	licence: string;
	/** Whether a roaster is in the middle of something a restart would cut (`GET /api/busy`) */
	busy: Busy;
	/** The dashboard's optional parts that are on (docs/lean-home.md) */
	modules: string[];
	/** home, enthusiast, roastery or custom */
	preset: string;
	/** Never set up and nothing recorded yet: the dashboard opens /welcome */
	first_run: boolean;
	/** The Modbus slave Artisan reads (docs/artisan-screen.md); null: off */
	artisan_screen: ArtisanScreen | null;
	/** The MQTT broker the server runs itself (docs/DEPLOY.md, "One container"); null: an outside broker */
	mqtt_broker: MqttBroker | null;
}

/** The server's built-in MQTT broker (ROASTWARDEN_BROKER_ADDR). */
interface MqttBroker {
	/** Where it listens, "0.0.0.0:1883" */
	addr: string;
	/** The roasters that may sign in (MQTT_DEVICES and Roasters → MQTT passwords), by device id */
	devices: string[];
}

/** The Modbus slave that lets Artisan be a screen for Roastwarden. */
interface ArtisanScreen {
	addr: string;
	port: number;
	/** The roaster it was told to serve; null: the one real roaster heard */
	roaster: string | null;
	/** The roaster it serves now; null: none, see problem */
	serving: string | null;
	problem: string | null;
	/** Writes become commands; false: read-only */
	writes: boolean;
	last_read_at: string | null;
	last_client: string | null;
}

/** The Artisan setup (.aset) that reads this server's Modbus slave. */
export const downloadArtisanScreen = (unit: 'C' | 'F') =>
	downloadFile(`/api/artisan/screen.aset?unit=${unit}`, 'Roastwarden.aset');

/** Where a build stands: the version moves only at a release. */
export interface Build {
	/** The newest release it contains ("0.4.0") */
	release?: string | null;
	/** Commits after that release: 0 is the release itself */
	commits_since_release?: number | null;
	/** When the commit was made */
	committed_at?: string | null;
}

/** What a roaster is doing that a server restart would cut. */
export type BusyReason = 'drive' | 'autotune' | 'cooling' | 'heating' | 'roasting';

export interface Busy {
	/** A real roaster is busy: a deploy waits */
	busy: boolean;
	roasters: {
		device_id: string;
		virtual: boolean;
		reasons: BusyReason[];
		bt_c: number | null;
		session_id: string | null;
	}[];
}

export const about = {
	get: () => request<AboutInfo>('/api/about')
};
