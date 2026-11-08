-- Device Configuration System
-- Migration: 003_device_configuration.sql

-- Device profiles table - reusable configuration templates for devices
CREATE TABLE IF NOT EXISTS device_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    default_control_mode TEXT DEFAULT 'manual',
    default_setpoint REAL,
    default_fan_pwm INTEGER,
    default_kp REAL,
    default_ki REAL,
    default_kd REAL,
    max_temp REAL DEFAULT 240,
    min_fan_pwm INTEGER DEFAULT 100,
    telemetry_interval_ms INTEGER DEFAULT 1000,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Devices table - persistent device configuration
CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    device_id TEXT NOT NULL UNIQUE,
    profile_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    description TEXT,
    location TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at DATETIME,
    FOREIGN KEY (profile_id) REFERENCES device_profiles(id) ON DELETE SET NULL
);

-- Device connections table - protocol-specific connection configurations
CREATE TABLE IF NOT EXISTS device_connections (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    protocol TEXT NOT NULL,
    enabled BOOLEAN DEFAULT 1,
    priority INTEGER DEFAULT 0,
    config JSON NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

-- Modbus register maps table - register definitions for Modbus TCP devices
CREATE TABLE IF NOT EXISTS modbus_register_maps (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    register_type TEXT NOT NULL,
    address INTEGER NOT NULL,
    name TEXT NOT NULL,
    data_type TEXT NOT NULL,
    byte_order TEXT DEFAULT 'ABCD',
    scale_factor REAL DEFAULT 1.0,
    offset REAL DEFAULT 0.0,
    unit TEXT,
    description TEXT,
    writable BOOLEAN DEFAULT 0,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_devices_device_id ON devices(device_id);
CREATE INDEX IF NOT EXISTS idx_device_connections_device_protocol ON device_connections(device_id, protocol);
CREATE INDEX IF NOT EXISTS idx_modbus_register_maps_device_id ON modbus_register_maps(device_id);

-- Triggers to update updated_at timestamp
CREATE TRIGGER IF NOT EXISTS update_devices_updated_at
    AFTER UPDATE ON devices
    FOR EACH ROW
BEGIN
    UPDATE devices SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_device_profiles_updated_at
    AFTER UPDATE ON device_profiles
    FOR EACH ROW
BEGIN
    UPDATE device_profiles SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_device_connections_updated_at
    AFTER UPDATE ON device_connections
    FOR EACH ROW
BEGIN
    UPDATE device_connections SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END