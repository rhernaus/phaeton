//! UI-oriented configuration schema for the Web UI
//!
//! This module builds a JSON schema structure expected by `webui/js/config.js`
//! to render an editable configuration form.

use serde_json::{Value, json};

/// Build the UI configuration schema consumed by the web UI
pub fn build_ui_schema() -> Value {
    json!({
        "sections": {
            "modbus": {"title": "Modbus", "type": "object", "fields": {
                "ip": {"type": "string", "format": "ipv4", "title": "Charger IP"},
                "port": {"type": "integer", "min": 1, "max": 65535, "title": "Port"},
                "socket_slave_id": {"type": "integer", "min": 1, "max": 247, "title": "Socket slave ID"},
                "station_slave_id": {"type": "integer", "min": 1, "max": 247, "title": "Station slave ID"}
            }},
            "defaults": {"title": "Defaults", "type": "object", "fields": {
                "intended_set_current": {"type": "number", "min": 0.0, "max": 80.0, "step": 0.1, "title": "Intended set current (A)"},
                "station_max_current": {"type": "number", "min": 0.0, "max": 80.0, "step": 0.1, "title": "Station max current (A)"}
            }},
            "controls": {"title": "Controls & Safety", "type": "object", "fields": {
                "current_tolerance": {"type": "number", "min": 0.0, "step": 0.01, "title": "Verification tolerance (A)"},
                "update_difference_threshold": {"type": "number", "min": 0.0, "step": 0.01, "title": "Update threshold (A)"},
                "verification_delay": {"type": "number", "min": 0.0, "step": 0.01, "title": "Verification delay (s)"},
                "retry_delay": {"type": "number", "min": 0.0, "step": 0.01, "title": "Retry delay (s)"},
                "max_retries": {"type": "integer", "min": 1, "title": "Max retries"},
                "watchdog_interval_seconds": {"type": "integer", "min": 1, "title": "Watchdog interval (s)"},
                "max_set_current": {"type": "number", "min": 0.01, "step": 0.1, "title": "Max set current (A)"},
                "min_set_current": {"type": "number", "min": 0.0, "step": 0.1, "title": "Min set current (A)"},
                "min_charge_duration_seconds": {"type": "integer", "min": 0, "title": "Min charge duration (s)"},
                "current_update_interval": {"type": "integer", "min": 0, "title": "Current update interval (ms)"},
                "verify_delay": {"type": "integer", "min": 0, "title": "Verify delay (ms)"},
                "ev_reporting_lag_ms": {"type": "integer", "min": 0, "title": "EV reporting lag (ms)"},
                "pv_excess_ema_alpha": {"type": "number", "min": 0.0, "max": 1.0, "step": 0.01, "title": "PV excess EMA alpha"}
            }},
            "logging": {"title": "Logging", "type": "object", "fields": {
                "level": {"type": "enum", "values": ["DEBUG","INFO","WARNING","ERROR","CRITICAL"], "title": "Level"},
                "file": {"type": "string", "title": "File path"},
                "format": {"type": "enum", "values": ["structured","simple"], "title": "Format"},
                "max_file_size_mb": {"type": "integer", "min": 1, "title": "Max file size (MB)"},
                "backup_count": {"type": "integer", "min": 0, "title": "Backups"},
                "console_output": {"type": "boolean", "title": "Console output"},
                "json_format": {"type": "boolean", "title": "JSON format"}
            }},
            "tibber": {"title": "Tibber (optional)", "type": "object", "fields": {
                "access_token": {"type": "string", "title": "Access token"},
                "home_id": {"type": "string", "title": "Home ID"},
                "charge_on_cheap": {"type": "boolean", "title": "Charge on CHEAP"},
                "charge_on_very_cheap": {"type": "boolean", "title": "Charge on VERY_CHEAP"},
                "strategy": {"type": "enum", "values": ["level","threshold","percentile"], "title": "Strategy"},
                "max_price_total": {"type": "number", "min": 0.0, "step": 0.001, "title": "Max price (threshold)"},
                "cheap_percentile": {"type": "number", "min": 0.0, "max": 1.0, "step": 0.01, "title": "Cheap percentile"}
            }},
            "vehicles": {"title": "Vehicles", "type": "list", "item": {"type": "object", "fields": {
                "name": {"type": "string", "title": "Name (optional)"},
                "provider": {"type": "enum", "values": ["tesla","kia"], "title": "Provider"},
                "poll_interval_seconds": {"type": "integer", "min": 10, "max": 86400, "title": "Polling interval (s)"},
                "tesla_access_token": {"type": "string", "title": "Tesla access token"},
                "tesla_vehicle_id": {"type": "integer", "title": "Tesla vehicle ID (optional)"},
                "tesla_vin": {"type": "string", "title": "Tesla VIN (optional)"},
                "tesla_wake_if_asleep": {"type": "boolean", "title": "Tesla wake if asleep"},
                "kia_username": {"type": "string", "title": "Kia username"},
                "kia_password": {"type": "string", "title": "Kia password"},
                "kia_pin": {"type": "string", "title": "Kia PIN"},
                "kia_region": {"type": "string", "title": "Region (EU/USA/CA/CN/AU)"},
                "kia_brand": {"type": "string", "title": "Brand (KIA/HYUNDAI)"},
                "kia_vin": {"type": "string", "title": "Kia VIN (optional)"}
            }}},
            "pricing": {"title": "Pricing", "type": "object", "fields": {
                "source": {"type": "enum", "values": ["victron","static"], "title": "Session cost source"},
                "static_rate_eur_per_kwh": {"type": "number", "min": 0.0, "step": 0.001, "title": "Static rate (EUR/kWh)"},
                "currency_symbol": {"type": "string", "title": "Currency symbol"}
            }},
            "schedule": {"title": "Schedules", "type": "object", "fields": {
                "mode": {"type": "enum", "values": ["time", "tibber"], "title": "Scheduling mode"},
                "items": {"type": "list", "item": {"type": "object", "fields": {
                    "active": {"type": "boolean", "title": "Active"},
                    "days": {"type": "array", "items": {"type": "integer", "min": 0, "max": 6}, "ui": "days", "title": "Days"},
                    "start_time": {"type": "time", "title": "Start time"},
                    "end_time": {"type": "time", "title": "End time"}
                }}}
            }},
            "registers": {"title": "Registers", "type": "object", "fields": {
                "voltages": {"type": "integer", "min": 0, "title": "Voltages base register"},
                "currents": {"type": "integer", "min": 0, "title": "Currents base register"},
                "power": {"type": "integer", "min": 0, "title": "Power register"},
                "energy": {"type": "integer", "min": 0, "title": "Energy register"},
                "status": {"type": "integer", "min": 0, "title": "Status string register"},
                "amps_config": {"type": "integer", "min": 0, "title": "Amps config register"},
                "phases": {"type": "integer", "min": 0, "title": "Phases register"},
                "firmware_version": {"type": "integer", "min": 0, "title": "Firmware version register"},
                "firmware_version_count": {"type": "integer", "min": 0, "title": "Firmware version count"},
                "station_serial": {"type": "integer", "min": 0, "title": "Station serial register"},
                "station_serial_count": {"type": "integer", "min": 0, "title": "Station serial count"},
                "manufacturer": {"type": "integer", "min": 0, "title": "Manufacturer register"},
                "manufacturer_count": {"type": "integer", "min": 0, "title": "Manufacturer count"},
                "platform_type": {"type": "integer", "min": 0, "title": "Platform type register"},
                "platform_type_count": {"type": "integer", "min": 0, "title": "Platform type count"},
                "station_max_current": {"type": "integer", "min": 0, "title": "Station max current (reg 1100)"},
                "station_status": {"type": "integer", "min": 0, "title": "Station status register"}
            }},
            "web": {"title": "Web UI", "type": "object", "fields": {
                "host": {"type": "string", "title": "Bind address"},
                "port": {"type": "integer", "min": 1, "max": 65535, "title": "Port"}
            }},
            "updates": {"title": "Updates", "type": "object", "fields": {
                "enabled": {"type": "boolean", "title": "Enable updater"},
                "auto_check": {"type": "boolean", "title": "Auto check"},
                "auto_update": {"type": "boolean", "title": "Auto update"},
                "include_prereleases": {"type": "boolean", "title": "Include prereleases"},
                "check_interval_hours": {"type": "integer", "min": 1, "max": 168, "title": "Check interval (h)"},
                "repository": {"type": "string", "title": "Repository URL (optional)"}
            }},
            "device_instance": {"title": "Device instance", "type": "integer", "min": 0, "max": 255},
            "require_dbus": {"title": "Require D-Bus on startup", "type": "boolean"},
            "poll_interval_ms": {"title": "Poll interval (ms)", "type": "integer", "min": 100, "max": 60000},
            "timezone": {"title": "Timezone", "type": "string"}
        }
    })
}
