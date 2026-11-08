-- A Modbus slave address per register (docs/artisan-machines.md): a bus of
-- several controllers on one RS-485 line (BT on slave 1, ET on slave 2, the burner on 3), as
-- most of Artisan's machine setups have. NULL: the connection's own unit_id, as before.
ALTER TABLE modbus_register_maps ADD COLUMN unit_id INTEGER;
