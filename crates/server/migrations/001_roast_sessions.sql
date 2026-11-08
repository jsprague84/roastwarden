-- Roast Sessions Management System
-- Migration: 001_roast_sessions.sql

-- Roast profiles table - defines reusable roasting profiles
CREATE TABLE roast_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_by TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_public BOOLEAN NOT NULL DEFAULT 0,
    
    -- Profile settings
    target_total_time INTEGER, -- seconds
    target_first_crack INTEGER, -- seconds from start
    target_end_temp REAL,
    preheat_temp REAL,
    charge_temp REAL
);

-- Profile points table - defines temperature curve points for profiles
CREATE TABLE profile_points (
    id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL,
    time_seconds INTEGER NOT NULL,
    target_temp REAL NOT NULL,
    fan_speed INTEGER, -- 0-100
    notes TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (profile_id) REFERENCES roast_profiles(id) ON DELETE CASCADE
);

-- Roast sessions table - individual roasting sessions
CREATE TABLE roast_sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    device_id TEXT NOT NULL,
    profile_id TEXT, -- Optional linked profile
    status TEXT NOT NULL DEFAULT 'planning',
    start_time DATETIME,
    end_time DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    -- Bean metadata
    bean_origin TEXT,
    bean_variety TEXT,
    green_weight REAL,
    roasted_weight REAL,
    target_roast_level TEXT,
    notes TEXT,
    
    -- Environmental conditions
    ambient_temp REAL,
    humidity REAL,
    
    -- Session summary data (calculated)
    max_temp REAL,
    total_time_seconds INTEGER,
    first_crack_time INTEGER, -- seconds from start
    development_time_ratio REAL, -- percentage of time after first crack
    
    FOREIGN KEY (profile_id) REFERENCES roast_profiles(id) ON DELETE SET NULL
);

-- Session telemetry table - time-series data for each session
CREATE TABLE session_telemetry (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    elapsed_seconds REAL NOT NULL,
    bean_temp REAL,
    env_temp REAL,
    rate_of_rise REAL,
    heater_pwm INTEGER,
    fan_pwm INTEGER,
    setpoint REAL,
    
    FOREIGN KEY (session_id) REFERENCES roast_sessions(id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX idx_roast_sessions_device_id ON roast_sessions(device_id);
CREATE INDEX idx_roast_sessions_status ON roast_sessions(status);
CREATE INDEX idx_roast_sessions_created_at ON roast_sessions(created_at);
CREATE INDEX idx_profile_points_profile_id ON profile_points(profile_id);
CREATE INDEX idx_profile_points_time ON profile_points(profile_id, time_seconds);
CREATE INDEX idx_session_telemetry_session_id ON session_telemetry(session_id);
CREATE INDEX idx_session_telemetry_timestamp ON session_telemetry(session_id, timestamp);
CREATE INDEX idx_roast_profiles_public ON roast_profiles(is_public);

-- Triggers to update updated_at timestamp
CREATE TRIGGER update_roast_sessions_updated_at
    AFTER UPDATE ON roast_sessions
    FOR EACH ROW
BEGIN
    UPDATE roast_sessions SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER update_roast_profiles_updated_at
    AFTER UPDATE ON roast_profiles
    FOR EACH ROW
BEGIN
    UPDATE roast_profiles SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- Insert some example profiles for testing
INSERT INTO roast_profiles (id, name, description, is_public, target_total_time, target_end_temp, charge_temp) VALUES
('default-light', 'Light Roast Profile', 'A gentle profile for light roast coffees with bright acidity', 1, 720, 205.0, 95.0),
('default-medium', 'Medium Roast Profile', 'Balanced profile for medium roast with good body and sweetness', 1, 840, 218.0, 95.0),
('default-dark', 'Dark Roast Profile', 'Bold profile for dark roast with rich, smoky flavors', 1, 960, 230.0, 95.0);

-- Insert example profile points for the light roast profile
INSERT INTO profile_points (id, profile_id, time_seconds, target_temp, fan_speed) VALUES
('light-p1', 'default-light', 0, 95.0, 50),
('light-p2', 'default-light', 60, 110.0, 55),
('light-p3', 'default-light', 180, 140.0, 60),
('light-p4', 'default-light', 360, 170.0, 65),
('light-p5', 'default-light', 540, 190.0, 70),
('light-p6', 'default-light', 660, 200.0, 75),
('light-p7', 'default-light', 720, 205.0, 80);

-- Insert example profile points for the medium roast profile  
INSERT INTO profile_points (id, profile_id, time_seconds, target_temp, fan_speed) VALUES
('medium-p1', 'default-medium', 0, 95.0, 45),
('medium-p2', 'default-medium', 90, 115.0, 50),
('medium-p3', 'default-medium', 240, 145.0, 55),
('medium-p4', 'default-medium', 420, 175.0, 60),
('medium-p5', 'default-medium', 600, 195.0, 65),
('medium-p6', 'default-medium', 750, 210.0, 70),
('medium-p7', 'default-medium', 840, 218.0, 75);