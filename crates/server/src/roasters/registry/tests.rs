//! Devices, device profiles, connections and register maps on a memory database.

use super::*;
use crate::roasters::types::{ModbusDataType, ModbusRegisterType, Protocol};

// ---- Device CRUD Tests ----

#[tokio::test]
async fn test_create_and_get_device() {
    let db = crate::db::open_memory().await;

    let req = CreateDeviceRequest {
        name: "Test Roaster".to_string(),
        device_id: "esp32-001".to_string(),
        profile_id: None,
        description: Some("A test device".to_string()),
        location: Some("Kitchen".to_string()),
    };

    let device = crate::roasters::registry::create_device(&db, req)
        .await
        .expect("create_device failed");
    assert_eq!(device.name, "Test Roaster");
    assert_eq!(device.device_id, "esp32-001");
    assert_eq!(device.status, DeviceStatus::Pending);
    assert_eq!(device.description, Some("A test device".to_string()));
    assert_eq!(device.location, Some("Kitchen".to_string()));

    // Get by id
    let fetched = crate::roasters::registry::get_device(&db, &device.id)
        .await
        .expect("get_device failed");
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.device.name, "Test Roaster");
    assert!(fetched.connections.is_empty());

    // Get by device_id
    let fetched = crate::roasters::registry::get_device_by_device_id(&db, "esp32-001")
        .await
        .expect("get_device_by_device_id failed");
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().device.id, device.id);
}

#[tokio::test]
async fn test_list_devices_with_status_filter() {
    let db = crate::db::open_memory().await;

    // Create two devices
    crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Device 1".to_string(),
            device_id: "d1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    let device2 = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Device 2".to_string(),
            device_id: "d2".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    // Activate device 2
    crate::roasters::registry::update_device_status(&db, &device2.id, DeviceStatus::Active)
        .await
        .unwrap();

    // List all
    let all = crate::roasters::registry::list_devices(&db, None)
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    // List only pending
    let pending = crate::roasters::registry::list_devices(&db, Some(DeviceStatus::Pending))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].device_id, "d1");

    // List only active
    let active = crate::roasters::registry::list_devices(&db, Some(DeviceStatus::Active))
        .await
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].device_id, "d2");
}

#[tokio::test]
async fn test_update_device() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Original".to_string(),
            device_id: "dev1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    let updated = crate::roasters::registry::update_device(
        &db,
        &device.id,
        UpdateDeviceRequest {
            name: Some("Updated Name".to_string()),
            profile_id: None,
            status: Some(DeviceStatus::Active),
            description: Some("Now with description".to_string()),
            location: None,
        },
    )
    .await
    .unwrap();

    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.status, DeviceStatus::Active);
    assert_eq!(
        updated.description,
        Some("Now with description".to_string())
    );
}

#[tokio::test]
async fn test_delete_device() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "To Delete".to_string(),
            device_id: "del1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    assert!(crate::roasters::registry::delete_device(&db, &device.id)
        .await
        .unwrap());
    assert!(!crate::roasters::registry::delete_device(&db, &device.id)
        .await
        .unwrap()); // Already deleted

    let fetched = crate::roasters::registry::get_device(&db, &device.id)
        .await
        .unwrap();
    assert!(fetched.is_none());
}

#[tokio::test]
async fn test_update_last_seen() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Seen Device".to_string(),
            device_id: "seen1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    assert!(device.last_seen_at.is_none());

    crate::roasters::registry::update_last_seen(&db, "seen1")
        .await
        .unwrap();

    let fetched = crate::roasters::registry::get_device(&db, &device.id)
        .await
        .unwrap()
        .unwrap();
    assert!(fetched.device.last_seen_at.is_some());
}

// ---- Device Profile Tests ----

#[tokio::test]
async fn test_create_and_list_profiles() {
    let db = crate::db::open_memory().await;

    let profile = crate::roasters::registry::create_device_profile(
        &db,
        CreateDeviceProfileRequest {
            name: "Default Profile".to_string(),
            description: Some("Standard roasting config".to_string()),
            default_control_mode: Some("auto".to_string()),
            default_setpoint: Some(200.0),
            default_fan: Some(70.0),
            default_kp: Some(15.0),
            default_ki: Some(1.0),
            default_kd: Some(25.0),
            max_temp: Some(240.0),
            min_fan: Some(40.0),
            telemetry_interval_ms: Some(1000),
        },
    )
    .await
    .unwrap();

    assert_eq!(profile.name, "Default Profile");
    assert_eq!(profile.default_setpoint, Some(200.0));

    let profiles = crate::roasters::registry::list_device_profiles(&db)
        .await
        .unwrap();
    assert_eq!(profiles.len(), 1);

    let fetched = crate::roasters::registry::get_device_profile(&db, &profile.id)
        .await
        .unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().name, "Default Profile");
}

#[tokio::test]
async fn test_delete_profile() {
    let db = crate::db::open_memory().await;

    let profile = crate::roasters::registry::create_device_profile(
        &db,
        CreateDeviceProfileRequest {
            name: "Temp Profile".to_string(),
            description: None,
            default_control_mode: None,
            default_setpoint: None,
            default_fan: None,
            default_kp: None,
            default_ki: None,
            default_kd: None,
            max_temp: None,
            min_fan: None,
            telemetry_interval_ms: None,
        },
    )
    .await
    .unwrap();

    assert!(
        crate::roasters::registry::delete_device_profile(&db, &profile.id)
            .await
            .unwrap()
    );
    assert!(
        !crate::roasters::registry::delete_device_profile(&db, &profile.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn test_device_with_profile() {
    let db = crate::db::open_memory().await;

    let profile = crate::roasters::registry::create_device_profile(
        &db,
        CreateDeviceProfileRequest {
            name: "Roast Profile".to_string(),
            description: None,
            default_control_mode: None,
            default_setpoint: Some(200.0),
            default_fan: None,
            default_kp: None,
            default_ki: None,
            default_kd: None,
            max_temp: None,
            min_fan: None,
            telemetry_interval_ms: None,
        },
    )
    .await
    .unwrap();

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Profiled Device".to_string(),
            device_id: "prof1".to_string(),
            profile_id: Some(profile.id.clone()),
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(device.profile_id, Some(profile.id));
}

// ---- Connection Tests ----

#[tokio::test]
async fn test_add_and_remove_connection() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Connected Device".to_string(),
            device_id: "conn1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    let mqtt_config = serde_json::json!({
        "topic_prefix": "roaster/conn1",
        "qos": 1
    });

    let conn = crate::roasters::registry::add_connection(
        &db,
        &device.id,
        CreateConnectionRequest {
            protocol: Protocol::Mqtt,
            enabled: Some(true),
            priority: Some(10),
            config: mqtt_config.clone(),
        },
    )
    .await
    .unwrap();

    assert_eq!(conn.protocol, Protocol::Mqtt);
    assert!(conn.enabled);
    assert_eq!(conn.priority, 10);
    assert_eq!(conn.config, mqtt_config);

    // Verify connection shows up in device fetch
    let fetched = crate::roasters::registry::get_device(&db, &device.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.connections.len(), 1);

    // Remove connection
    assert!(crate::roasters::registry::remove_connection(&db, &conn.id)
        .await
        .unwrap());
    assert!(!crate::roasters::registry::remove_connection(&db, &conn.id)
        .await
        .unwrap());

    let fetched = crate::roasters::registry::get_device(&db, &device.id)
        .await
        .unwrap()
        .unwrap();
    assert!(fetched.connections.is_empty());
}

#[tokio::test]
async fn test_update_connection() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Update Conn Device".to_string(),
            device_id: "uconn1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    let conn = crate::roasters::registry::add_connection(&db, &device.id, CreateConnectionRequest {
        protocol: Protocol::WebSocket,
        enabled: Some(true),
        priority: Some(0),
        config: serde_json::json!({"url": "ws://10.0.0.1:8080/ws", "reconnect_interval_ms": 5000}),
    }).await.unwrap();

    let updated = crate::roasters::registry::update_connection(
        &db,
        &conn.id,
        UpdateConnectionRequest {
            enabled: Some(false),
            priority: Some(5),
            config: None,
        },
    )
    .await
    .unwrap();

    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.priority, 5);
}

#[tokio::test]
async fn test_cascade_delete_connections() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Cascade Device".to_string(),
            device_id: "casc1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    crate::roasters::registry::add_connection(
        &db,
        &device.id,
        CreateConnectionRequest {
            protocol: Protocol::Mqtt,
            enabled: None,
            priority: None,
            config: serde_json::json!({"topic_prefix": "roaster/casc1", "qos": 0}),
        },
    )
    .await
    .unwrap();

    crate::roasters::registry::add_connection(&db, &device.id, CreateConnectionRequest {
        protocol: Protocol::ModbusTcp,
        enabled: None,
        priority: None,
        config: serde_json::json!({"host": "10.0.0.1", "port": 502, "unit_id": 1, "poll_interval_ms": 1000}),
    }).await.unwrap();

    // Delete device should cascade delete connections
    assert!(crate::roasters::registry::delete_device(&db, &device.id)
        .await
        .unwrap());

    // Connections should be gone (we can verify by trying to get the device)
    let fetched = crate::roasters::registry::get_device(&db, &device.id)
        .await
        .unwrap();
    assert!(fetched.is_none());
}

// ---- Register Map Tests ----

#[tokio::test]
async fn test_set_and_get_register_map() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Modbus Device".to_string(),
            device_id: "modbus1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    let registers = vec![
        CreateRegisterMapEntry {
            register_type: ModbusRegisterType::Input,
            address: 0,
            name: "bean_temp_hi".to_string(),
            data_type: ModbusDataType::Float32,
            byte_order: Some("ABCD".to_string()),
            scale_factor: Some(1.0),
            offset: Some(0.0),
            unit: Some("C".to_string()),
            description: Some("Bean temperature (high word)".to_string()),
            writable: Some(false),
            role: None,
            unit_id: None,
        },
        CreateRegisterMapEntry {
            register_type: ModbusRegisterType::Holding,
            address: 0,
            name: "setpoint".to_string(),
            data_type: ModbusDataType::Float32,
            byte_order: Some("ABCD".to_string()),
            scale_factor: Some(1.0),
            offset: Some(0.0),
            unit: Some("C".to_string()),
            description: Some("Target setpoint".to_string()),
            writable: Some(true),
            role: None,
            unit_id: None,
        },
    ];

    let result = crate::roasters::registry::set_register_map(&db, &device.id, registers)
        .await
        .unwrap();
    assert_eq!(result.len(), 2);

    let fetched = crate::roasters::registry::get_register_map(&db, &device.id)
        .await
        .unwrap();
    assert_eq!(fetched.len(), 2);
    // Ordered by register_type, address
    assert_eq!(fetched[0].name, "setpoint"); // holding comes before input alphabetically
    assert_eq!(fetched[1].name, "bean_temp_hi");
}

#[tokio::test]
async fn test_set_register_map_replaces_existing() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Replace Map Device".to_string(),
            device_id: "rmap1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    // Set initial map
    crate::roasters::registry::set_register_map(
        &db,
        &device.id,
        vec![CreateRegisterMapEntry {
            register_type: ModbusRegisterType::Input,
            address: 0,
            name: "old_register".to_string(),
            data_type: ModbusDataType::Uint16,
            byte_order: None,
            scale_factor: None,
            offset: None,
            unit: None,
            description: None,
            writable: None,
            role: None,
            unit_id: None,
        }],
    )
    .await
    .unwrap();

    // Replace with new map
    crate::roasters::registry::set_register_map(
        &db,
        &device.id,
        vec![
            CreateRegisterMapEntry {
                register_type: ModbusRegisterType::Holding,
                address: 0,
                name: "new_register_1".to_string(),
                data_type: ModbusDataType::Float32,
                byte_order: None,
                scale_factor: None,
                offset: None,
                unit: None,
                description: None,
                writable: None,
                role: None,
                unit_id: None,
            },
            CreateRegisterMapEntry {
                register_type: ModbusRegisterType::Holding,
                address: 2,
                name: "new_register_2".to_string(),
                data_type: ModbusDataType::Uint16,
                byte_order: None,
                scale_factor: None,
                offset: None,
                unit: None,
                description: None,
                writable: None,
                role: None,
                unit_id: None,
            },
        ],
    )
    .await
    .unwrap();

    let fetched = crate::roasters::registry::get_register_map(&db, &device.id)
        .await
        .unwrap();
    assert_eq!(fetched.len(), 2);
    // Old register should be gone
    assert!(fetched.iter().all(|r| r.name != "old_register"));
    assert!(fetched.iter().any(|r| r.name == "new_register_1"));
    assert!(fetched.iter().any(|r| r.name == "new_register_2"));
}

#[tokio::test]
async fn test_cascade_delete_register_maps() {
    let db = crate::db::open_memory().await;

    let device = crate::roasters::registry::create_device(
        &db,
        CreateDeviceRequest {
            name: "Cascade Reg Device".to_string(),
            device_id: "creg1".to_string(),
            profile_id: None,
            description: None,
            location: None,
        },
    )
    .await
    .unwrap();

    crate::roasters::registry::set_register_map(
        &db,
        &device.id,
        vec![CreateRegisterMapEntry {
            register_type: ModbusRegisterType::Input,
            address: 0,
            name: "temp".to_string(),
            data_type: ModbusDataType::Float32,
            byte_order: None,
            scale_factor: None,
            offset: None,
            unit: None,
            description: None,
            writable: None,
            role: None,
            unit_id: None,
        }],
    )
    .await
    .unwrap();

    // Delete device should cascade delete register maps
    crate::roasters::registry::delete_device(&db, &device.id)
        .await
        .unwrap();

    // Verify register maps are gone
    let registers = crate::roasters::registry::get_register_map(&db, &device.id)
        .await
        .unwrap();
    assert!(registers.is_empty());
}
