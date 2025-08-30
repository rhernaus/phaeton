use super::*;

impl Default for ModbusConfig {
    fn default() -> Self {
        Self {
            ip: "192.168.1.100".to_string(),
            port: 502,
            socket_slave_id: 1,
            station_slave_id: 200,
        }
    }
}

impl Default for RegistersConfig {
    fn default() -> Self {
        Self {
            voltages: 306,
            currents: 320,
            power: 338,
            energy: 374,
            status: 1201,
            amps_config: 1210,
            phases: 1215,
            firmware_version: 123,
            firmware_version_count: 17,
            station_serial: 157,
            station_serial_count: 11,
            manufacturer: 117,
            manufacturer_count: 5,
            platform_type: 140,
            platform_type_count: 17,
            station_max_current: 1100,
            station_status: 1201,
        }
    }
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            intended_set_current: 6.0,
            station_max_current: 32.0,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "INFO".to_string(),
            console_level: None,
            file_level: None,
            web_level: None,
            file: "/tmp/phaeton.log".to_string(),
            format: "structured".to_string(),
            max_file_size_mb: 10,
            backup_count: 5,
            console_output: true,
            json_format: false,
        }
    }
}

impl Default for TibberConfig {
    fn default() -> Self {
        Self {
            access_token: String::new(),
            home_id: String::new(),
            charge_on_cheap: true,
            charge_on_very_cheap: true,
            strategy: "level".to_string(),
            max_price_total: 0.0,
            cheap_percentile: 0.3,
        }
    }
}

impl Default for ControlsConfig {
    fn default() -> Self {
        Self {
            current_tolerance: 0.5,
            update_difference_threshold: 0.1,
            verification_delay: 0.1,
            retry_delay: 1.0,
            max_retries: 10,
            watchdog_interval_seconds: 30,
            max_set_current: 64.0,
            min_set_current: 6.0,
            min_charge_duration_seconds: 300,
            current_update_interval: 30000,
            verify_delay: 100,
            ev_reporting_lag_ms: 2000,
            pv_excess_ema_alpha: 0.4,
            phase_switch_grace_seconds: 60,
            phase_switch_settle_seconds: 5,
            auto_phase_switch: true,
            auto_phase_hysteresis_watts: 300.0,
        }
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8088,
        }
    }
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            source: "static".to_string(),
            static_rate_eur_per_kwh: 0.25,
            currency_symbol: "€".to_string(),
        }
    }
}

impl Default for UpdaterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_check: true,
            auto_update: false,
            include_prereleases: false,
            check_interval_hours: 24,
            repository: String::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            modbus: ModbusConfig::default(),
            device_instance: 0,
            require_dbus: true,
            registers: RegistersConfig::default(),
            defaults: DefaultsConfig::default(),
            logging: LoggingConfig::default(),
            schedule: ScheduleConfig::default(),
            tibber: TibberConfig::default(),
            controls: ControlsConfig::default(),
            poll_interval_ms: 1000,
            timezone: "UTC".to_string(),
            web: WebConfig::default(),
            pricing: PricingConfig::default(),
            updates: UpdaterConfig::default(),
            vehicles: None,
        }
    }
}
