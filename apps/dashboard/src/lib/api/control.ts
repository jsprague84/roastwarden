/**
 * Control commands to a roaster (docs/control.md): setpoint, fan, heater, drum, damper, mode, PID
 * gains, emergency stop.
 */

import { request } from '#lib/api/http.js';

export interface ControlApi {
	setSetpoint(deviceId: string, value: number): Promise<void>;
	/** Fan, 0-100 %. */
	setFan(deviceId: string, percent: number): Promise<void>;
	/** Manual heater power, 0-100 %. */
	setHeater(deviceId: string, percent: number): Promise<void>;
	/** Drum speed, 0-100 % (a declared drum output only). */
	setDrum(deviceId: string, percent: number): Promise<void>;
	/** Damper, 0-100 % (a declared damper output only). */
	setDamper(deviceId: string, percent: number): Promise<void>;
	setMode(deviceId: string, mode: 'manual' | 'auto'): Promise<void>;
	setHeaterEnable(deviceId: string, enabled: boolean): Promise<void>;
	setPid(deviceId: string, kp: number, ki: number, kd: number): Promise<void>;
	emergencyStop(deviceId: string): Promise<void>;
}

export const control: ControlApi = {
	setSetpoint: (deviceId, value) =>
		request(
			`/api/roaster/${deviceId}/control/setpoint`,
			{
				method: 'POST',
				body: JSON.stringify({ value })
			},
			true
		),

	setFan: (deviceId, percent) =>
		request(
			`/api/roaster/${deviceId}/control/fan`,
			{
				method: 'POST',
				body: JSON.stringify({ percent })
			},
			true
		),

	setHeater: (deviceId, percent) =>
		request(
			`/api/roaster/${deviceId}/control/heater`,
			{
				method: 'POST',
				body: JSON.stringify({ percent })
			},
			true
		),

	setDrum: (deviceId, percent) =>
		request(
			`/api/roaster/${deviceId}/control/drum`,
			{
				method: 'POST',
				body: JSON.stringify({ percent })
			},
			true
		),

	setDamper: (deviceId, percent) =>
		request(
			`/api/roaster/${deviceId}/control/damper`,
			{
				method: 'POST',
				body: JSON.stringify({ percent })
			},
			true
		),

	setMode: (deviceId, mode) =>
		request(
			`/api/roaster/${deviceId}/control/mode`,
			{
				method: 'POST',
				body: JSON.stringify({ mode })
			},
			true
		),

	setHeaterEnable: (deviceId, enabled) =>
		request(
			`/api/roaster/${deviceId}/control/heater_enable`,
			{
				method: 'POST',
				body: JSON.stringify({ enabled })
			},
			true
		),

	setPid: (deviceId, kp, ki, kd) =>
		request(
			`/api/roaster/${deviceId}/control/pid`,
			{
				method: 'POST',
				body: JSON.stringify({ kp, ki, kd })
			},
			true
		),

	emergencyStop: (deviceId) =>
		request(
			`/api/roaster/${deviceId}/control/emergency_stop`,
			{
				method: 'POST'
			},
			true
		)
};
