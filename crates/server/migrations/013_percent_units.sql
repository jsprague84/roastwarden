-- Migration: 013_percent_units.sql
-- Heater and fan are percent everywhere (firmware 3.3 speaks % on MQTT).
-- Session telemetry stored the fan as 0-255 PWM; device profiles stored fan
-- defaults the same way. Convert them, and drop the raw fan column the
-- autotune runs kept for firmware that reported 0-255.

ALTER TABLE session_telemetry ADD COLUMN heater REAL;
ALTER TABLE session_telemetry ADD COLUMN fan REAL;
UPDATE session_telemetry
   SET heater = heater_pwm,
       fan = ROUND(fan_pwm * 100.0 / 255.0, 1);
ALTER TABLE session_telemetry DROP COLUMN heater_pwm;
ALTER TABLE session_telemetry DROP COLUMN fan_pwm;

ALTER TABLE device_profiles ADD COLUMN default_fan REAL;
ALTER TABLE device_profiles ADD COLUMN min_fan REAL;
UPDATE device_profiles
   SET default_fan = ROUND(default_fan_pwm * 100.0 / 255.0, 1),
       min_fan = ROUND(min_fan_pwm * 100.0 / 255.0, 1);
ALTER TABLE device_profiles DROP COLUMN default_fan_pwm;
ALTER TABLE device_profiles DROP COLUMN min_fan_pwm;

ALTER TABLE autotune_runs DROP COLUMN fan_native;
