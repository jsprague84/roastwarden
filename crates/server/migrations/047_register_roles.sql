-- A role per Modbus register (docs/ingest-mapping.md, "Modbus TCP and RTU"):
-- `bt`, `et`, `ambient`, `ror`, `heater`, `heater_applied`, `fan`, `drum`, `damper`, `setpoint`,
-- `mode`, `heater_enable` or `emergency_stop`. A register with a role is read as the role's field
-- and written by the command of that role; NULL keeps today's behaviour (its name).
ALTER TABLE modbus_register_maps ADD COLUMN role TEXT;
