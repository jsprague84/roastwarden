//! The roasters' records (`devices`), their connections, device profiles and Modbus register maps.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::roasters::types::{
    CreateConnectionRequest, CreateDeviceProfileRequest, CreateDeviceRequest,
    CreateRegisterMapEntry, Device, DeviceConnection, DeviceProfile, DeviceStatus,
    DeviceWithConnections, ModbusRegisterMap, UpdateConnectionRequest, UpdateDeviceProfileRequest,
    UpdateDeviceRequest,
};

// ---- Device CRUD ----

pub async fn list_devices(
    db: &SqlitePool,
    status_filter: Option<DeviceStatus>,
) -> Result<Vec<Device>> {
    if let Some(status) = status_filter {
        let devices = sqlx::query_as::<_, Device>(
            "SELECT * FROM devices WHERE status = ? ORDER BY created_at DESC",
        )
        .bind(status.to_string())
        .fetch_all(db)
        .await?;
        Ok(devices)
    } else {
        let devices = sqlx::query_as::<_, Device>("SELECT * FROM devices ORDER BY created_at DESC")
            .fetch_all(db)
            .await?;
        Ok(devices)
    }
}

pub async fn get_device(db: &SqlitePool, id: &str) -> Result<Option<DeviceWithConnections>> {
    let device = sqlx::query_as::<_, Device>("SELECT * FROM devices WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;

    let Some(device) = device else {
        return Ok(None);
    };

    let connections = sqlx::query_as::<_, DeviceConnection>(
        "SELECT * FROM device_connections WHERE device_id = ? ORDER BY priority DESC",
    )
    .bind(id)
    .fetch_all(db)
    .await?;

    Ok(Some(DeviceWithConnections {
        device,
        connections,
    }))
}

pub async fn get_device_by_device_id(
    db: &SqlitePool,
    device_id: &str,
) -> Result<Option<DeviceWithConnections>> {
    let device = sqlx::query_as::<_, Device>("SELECT * FROM devices WHERE device_id = ?")
        .bind(device_id)
        .fetch_optional(db)
        .await?;

    let Some(device) = device else {
        return Ok(None);
    };

    let connections = sqlx::query_as::<_, DeviceConnection>(
        "SELECT * FROM device_connections WHERE device_id = ? ORDER BY priority DESC",
    )
    .bind(&device.id)
    .fetch_all(db)
    .await?;

    Ok(Some(DeviceWithConnections {
        device,
        connections,
    }))
}

pub async fn create_device(db: &SqlitePool, req: CreateDeviceRequest) -> Result<Device> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let device = sqlx::query_as::<_, Device>(
            r"
            INSERT INTO devices (id, name, device_id, profile_id, status, description, location, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            "
        )
        .bind(&id)
        .bind(&req.name)
        .bind(&req.device_id)
        .bind(&req.profile_id)
        .bind(DeviceStatus::Pending.to_string())
        .bind(&req.description)
        .bind(&req.location)
        .bind(now)
        .bind(now)
        .fetch_all(db)
        .await?
        .pop()
        .context("RETURNING gave no row")?;

    Ok(device)
}

pub async fn update_device(
    db: &SqlitePool,
    id: &str,
    req: UpdateDeviceRequest,
) -> Result<Option<Device>> {
    if req.name.is_none()
        && req.profile_id.is_none()
        && req.status.is_none()
        && req.description.is_none()
        && req.location.is_none()
    {
        // Nothing to update, just return the existing device
        let device = sqlx::query_as::<_, Device>("SELECT * FROM devices WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?;
        return Ok(device);
    }

    let now = Utc::now();
    let mut query = "UPDATE devices SET updated_at = ?".to_string();

    if req.name.is_some() {
        query.push_str(", name = ?");
    }
    if req.profile_id.is_some() {
        query.push_str(", profile_id = ?");
    }
    if req.status.is_some() {
        query.push_str(", status = ?");
    }
    if req.description.is_some() {
        query.push_str(", description = ?");
    }
    if req.location.is_some() {
        query.push_str(", location = ?");
    }

    query.push_str(" WHERE id = ? RETURNING *");

    let mut query_builder = sqlx::query_as::<_, Device>(&query).bind(now);

    if let Some(ref name) = req.name {
        query_builder = query_builder.bind(name);
    }
    if let Some(ref profile_id) = req.profile_id {
        query_builder = query_builder.bind(profile_id);
    }
    if let Some(ref status) = req.status {
        query_builder = query_builder.bind(status.to_string());
    }
    if let Some(ref description) = req.description {
        query_builder = query_builder.bind(description);
    }
    if let Some(ref location) = req.location {
        query_builder = query_builder.bind(location);
    }

    query_builder = query_builder.bind(id);

    let device = query_builder.fetch_all(db).await?.pop();
    Ok(device)
}

pub async fn delete_device(db: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;

    Ok(result.rows_affected() > 0)
}

#[allow(dead_code)] // Will be used by device status transitions
pub async fn update_device_status(
    db: &SqlitePool,
    id: &str,
    status: DeviceStatus,
) -> Result<Option<Device>> {
    let device = sqlx::query_as::<_, Device>(
        "UPDATE devices SET status = ?, updated_at = ? WHERE id = ? RETURNING *",
    )
    .bind(status.to_string())
    .bind(Utc::now())
    .bind(id)
    .fetch_all(db)
    .await?
    .pop();

    Ok(device)
}

pub async fn update_last_seen(db: &SqlitePool, device_id: &str) -> Result<()> {
    sqlx::query("UPDATE devices SET last_seen_at = ? WHERE device_id = ?")
        .bind(Utc::now())
        .bind(device_id)
        .execute(db)
        .await?;

    Ok(())
}

// ---- Device Profile CRUD ----

pub async fn list_device_profiles(db: &SqlitePool) -> Result<Vec<DeviceProfile>> {
    let profiles = sqlx::query_as::<_, DeviceProfile>(
        "SELECT * FROM device_profiles ORDER BY created_at DESC",
    )
    .fetch_all(db)
    .await?;

    Ok(profiles)
}

pub async fn get_device_profile(db: &SqlitePool, id: &str) -> Result<Option<DeviceProfile>> {
    let profile = sqlx::query_as::<_, DeviceProfile>("SELECT * FROM device_profiles WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;

    Ok(profile)
}

pub async fn create_device_profile(
    db: &SqlitePool,
    req: CreateDeviceProfileRequest,
) -> Result<DeviceProfile> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let profile = sqlx::query_as::<_, DeviceProfile>(
        r"
            INSERT INTO device_profiles (
                id, name, description, default_control_mode, default_setpoint, default_fan,
                default_kp, default_ki, default_kd, max_temp, min_fan, telemetry_interval_ms,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            ",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.default_control_mode)
    .bind(req.default_setpoint)
    .bind(req.default_fan)
    .bind(req.default_kp)
    .bind(req.default_ki)
    .bind(req.default_kd)
    .bind(req.max_temp)
    .bind(req.min_fan)
    .bind(req.telemetry_interval_ms)
    .bind(now)
    .bind(now)
    .fetch_all(db)
    .await?
    .pop()
    .context("RETURNING gave no row")?;

    Ok(profile)
}

pub async fn update_device_profile(
    db: &SqlitePool,
    id: &str,
    req: UpdateDeviceProfileRequest,
) -> Result<Option<DeviceProfile>> {
    let now = Utc::now();
    let profile = sqlx::query_as::<_, DeviceProfile>(
        r"
            UPDATE device_profiles SET
                name = COALESCE(?, name),
                description = COALESCE(?, description),
                default_control_mode = COALESCE(?, default_control_mode),
                default_setpoint = COALESCE(?, default_setpoint),
                default_fan = COALESCE(?, default_fan),
                default_kp = COALESCE(?, default_kp),
                default_ki = COALESCE(?, default_ki),
                default_kd = COALESCE(?, default_kd),
                max_temp = COALESCE(?, max_temp),
                min_fan = COALESCE(?, min_fan),
                telemetry_interval_ms = COALESCE(?, telemetry_interval_ms),
                updated_at = ?
            WHERE id = ?
            RETURNING *
            ",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.default_control_mode)
    .bind(req.default_setpoint)
    .bind(req.default_fan)
    .bind(req.default_kp)
    .bind(req.default_ki)
    .bind(req.default_kd)
    .bind(req.max_temp)
    .bind(req.min_fan)
    .bind(req.telemetry_interval_ms)
    .bind(now)
    .bind(id)
    .fetch_all(db)
    .await?
    .pop();

    Ok(profile)
}

pub async fn delete_device_profile(db: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM device_profiles WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ---- Device Connection CRUD ----

pub async fn add_connection(
    db: &SqlitePool,
    device_id: &str,
    req: CreateConnectionRequest,
) -> Result<DeviceConnection> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let connection = sqlx::query_as::<_, DeviceConnection>(
            r"
            INSERT INTO device_connections (id, device_id, protocol, enabled, priority, config, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *
            "
        )
        .bind(&id)
        .bind(device_id)
        .bind(req.protocol.to_string())
        .bind(req.enabled.unwrap_or(true))
        .bind(req.priority.unwrap_or(0))
        .bind(serde_json::to_string(&req.config)?)
        .bind(now)
        .bind(now)
        .fetch_all(db)
        .await?
        .pop()
        .context("RETURNING gave no row")?;

    Ok(connection)
}

pub async fn update_connection(
    db: &SqlitePool,
    connection_id: &str,
    req: UpdateConnectionRequest,
) -> Result<Option<DeviceConnection>> {
    if req.enabled.is_none() && req.priority.is_none() && req.config.is_none() {
        let conn =
            sqlx::query_as::<_, DeviceConnection>("SELECT * FROM device_connections WHERE id = ?")
                .bind(connection_id)
                .fetch_optional(db)
                .await?;
        return Ok(conn);
    }

    let now = Utc::now();
    let mut query = "UPDATE device_connections SET updated_at = ?".to_string();

    if req.enabled.is_some() {
        query.push_str(", enabled = ?");
    }
    if req.priority.is_some() {
        query.push_str(", priority = ?");
    }
    if req.config.is_some() {
        query.push_str(", config = ?");
    }

    query.push_str(" WHERE id = ? RETURNING *");

    let mut query_builder = sqlx::query_as::<_, DeviceConnection>(&query).bind(now);

    if let Some(enabled) = req.enabled {
        query_builder = query_builder.bind(enabled);
    }
    if let Some(priority) = req.priority {
        query_builder = query_builder.bind(priority);
    }
    if let Some(ref config) = req.config {
        query_builder = query_builder.bind(serde_json::to_string(config)?);
    }

    query_builder = query_builder.bind(connection_id);

    let connection = query_builder.fetch_all(db).await?.pop();
    Ok(connection)
}

pub async fn get_connection(
    db: &SqlitePool,
    connection_id: &str,
) -> Result<Option<DeviceConnection>> {
    Ok(
        sqlx::query_as::<_, DeviceConnection>("SELECT * FROM device_connections WHERE id = ?")
            .bind(connection_id)
            .fetch_optional(db)
            .await?,
    )
}

pub async fn remove_connection(db: &SqlitePool, connection_id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM device_connections WHERE id = ?")
        .bind(connection_id)
        .execute(db)
        .await?;

    Ok(result.rows_affected() > 0)
}

// ---- Register Map CRUD ----

pub async fn get_register_map(db: &SqlitePool, device_id: &str) -> Result<Vec<ModbusRegisterMap>> {
    let registers = sqlx::query_as::<_, ModbusRegisterMap>(
        "SELECT * FROM modbus_register_maps WHERE device_id = ? ORDER BY register_type, address",
    )
    .bind(device_id)
    .fetch_all(db)
    .await?;

    Ok(registers)
}

pub async fn set_register_map(
    db: &SqlitePool,
    device_id: &str,
    registers: Vec<CreateRegisterMapEntry>,
) -> Result<Vec<ModbusRegisterMap>> {
    // Delete existing register map and insert new entries in a transaction
    let mut tx = db.begin().await?;

    sqlx::query("DELETE FROM modbus_register_maps WHERE device_id = ?")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;

    let mut result = Vec::new();
    for entry in registers {
        let id = Uuid::new_v4().to_string();
        let register = sqlx::query_as::<_, ModbusRegisterMap>(
            r"
                INSERT INTO modbus_register_maps (
                    id, device_id, register_type, address, name, data_type,
                    byte_order, scale_factor, offset, unit, description, writable, role,
                    unit_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                RETURNING *
                ",
        )
        .bind(&id)
        .bind(device_id)
        .bind(entry.register_type.to_string())
        .bind(entry.address)
        .bind(&entry.name)
        .bind(entry.data_type.to_string())
        .bind(&entry.byte_order)
        .bind(entry.scale_factor)
        .bind(entry.offset)
        .bind(&entry.unit)
        .bind(&entry.description)
        .bind(entry.writable.unwrap_or(false))
        .bind(
            entry
                .role
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty()),
        )
        .bind(entry.unit_id)
        .fetch_one(&mut *tx)
        .await?;

        result.push(register);
    }

    tx.commit().await?;
    Ok(result)
}

#[cfg(test)]
mod tests;
