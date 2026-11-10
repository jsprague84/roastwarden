//! A machine on Fuji PIDs (Artisan device 0): Artisan's own Fuji protocol is Modbus RTU,
//! so a register map from each controller's process value, output and setpoint.

use crate::roasters::channels::Channel;
use crate::roasters::setup::{Kind, Outputs};
use crate::roasters::types::{CreateRegisterMapEntry, ModbusDataType, ModbusRegisterType};

use super::inputs::Reads;
use super::{Plan, finish, serial_config};
use crate::roasters::aset::ini::Ini;
use crate::roasters::aset::machines::{fuji, fuji_pid};

/// A machine on Fuji PIDs (Artisan device 0): Artisan's own Fuji protocol is Modbus RTU, so a
/// register map: each controller's process value, the ET controller's output and setpoint.
pub(super) fn plan_fuji(ini: &Ini, plan: &mut Plan) {
    let Some((et_type, et_unit)) = fuji_pid(ini, "controlETpid") else {
        return;
    };
    let Some(etc) = fuji(et_type) else {
        return;
    };
    let btc = fuji_pid(ini, "readBTpid").and_then(|(t, u)| fuji(t).map(|f| (f, u)));
    let unit = btc.map_or(et_unit, |(_, u)| u);
    let own = |u: u8| (u != unit).then_some(u as i64);
    plan.config = Some(serial_config(ini, "SerialPort", unit, false, plan));
    let mut reads = Reads::default();
    let entry = |kind: ModbusRegisterType,
                 address: i32,
                 name: &str,
                 data_type: ModbusDataType,
                 scale: f64,
                 temp: bool,
                 what: String,
                 role: Option<&str>,
                 slave: u8| CreateRegisterMapEntry {
        register_type: kind,
        address,
        name: name.into(),
        data_type,
        byte_order: None,
        scale_factor: Some(scale),
        offset: None,
        unit: temp.then(|| "C".to_string()),
        description: Some(what),
        writable: Some(kind == ModbusRegisterType::Holding),
        role: role.map(str::to_string),
        unit_id: own(slave),
    };
    let mut registers = Vec::new();
    let input = ModbusRegisterType::Input;
    match btc {
        Some((b, u)) => registers.push(entry(
            input,
            b.pv,
            "BT",
            ModbusDataType::Int16,
            0.1,
            true,
            format!("Fuji {} PV (slave {u})", b.model),
            Some("bt"),
            u,
        )),
        None => plan
            .notes
            .push("no BT controller in the setup: ET is read alone".into()),
    }
    registers.push(entry(
        input,
        etc.pv,
        "ET",
        ModbusDataType::Int16,
        0.1,
        true,
        format!("Fuji {} PV (slave {et_unit})", etc.model),
        Some("et"),
        et_unit,
    ));
    registers.push(entry(
        input,
        etc.mv,
        "ET PID output",
        ModbusDataType::Int16,
        0.01,
        false,
        format!("Fuji {} MV1, −3 to 103 % (slave {et_unit})", etc.model),
        Some("heater"),
        et_unit,
    ));
    reads
        .used
        .extend(["BT", "ET", "ET PID output", "ET SV"].map(String::from));
    reads.channels.push(Channel {
        key: "et_sv".into(),
        label: "ET SV".into(),
        unit: "C".into(),
        max: None,
    });
    registers.push(entry(
        input,
        etc.sv,
        "et_sv",
        ModbusDataType::Int16,
        0.1,
        true,
        format!("ET SV: Fuji {} SV in use (slave {et_unit})", etc.model),
        None,
        et_unit,
    ));
    registers.push(entry(
        ModbusRegisterType::Holding,
        etc.sv_write,
        "ET SV write",
        ModbusDataType::Uint16,
        0.1,
        true,
        format!(
            "Fuji {} {} (slave {et_unit}), the setpoint Artisan writes",
            etc.model, etc.sv_name
        ),
        Some("setpoint"),
        et_unit,
    ));
    let models = match btc {
        Some((b, _)) if b.model != etc.model => format!("{} (ET) and {} (BT)", etc.model, b.model),
        _ => etc.model.to_string(),
    };
    plan.notes.extend([
        format!(
            "Fuji {models} controllers: Artisan's Fuji protocol is Modbus RTU; each PV is input \
             register {} × 0.1, the controller's decimal point at 1 as Artisan reads it",
            etc.pv
        ),
        format!(
            "the ET controller's output (MV1) is read as the heater, its {} (holding register \
             {}) is Roastwarden's setpoint: an ET target, not BT's; Artisan's PID dialog and \
             ramp/soak are not imported",
            etc.sv_name, etc.sv_write
        ),
        "temperatures as the controllers show them, in °C; a controller set to °F: set those \
         registers' unit to F"
            .into(),
    ]);
    plan.registers = registers;
    finish(ini, plan, Outputs::default(), reads.channels, Kind::DrumGas);
}
