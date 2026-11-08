-- A roast keeps what the roaster said about its control with each sample:
-- the applied heater (after the RoR limiter and interlock), the RoR limiter flag, the PID
-- gains in use, the control mode, heater power and the latched stop code. Before, these
-- lived only in the 2 h ring, so a roast's gains as the fan moved, or why its heater was cut,
-- could not be looked at afterwards. JSON with only the keys the device sent.
ALTER TABLE session_telemetry ADD COLUMN extras TEXT;
