use std::sync::Mutex;
use tokio::sync::mpsc;

use crate::driver::DriverCommand;

#[derive(Default)]
pub struct EvChargerValues {
    pub device_instance: u32,
    pub product_name: String,
    pub firmware_version: String,
    pub serial: String,
    pub product_id: u32,
    pub connected: u8,
    pub mode: u8,
    pub start_stop: u8,
    pub set_current: f64,
    pub max_current: f64,
    pub current: f64,
    pub ac_power: f64,
    pub ac_energy_forward: f64,
    pub ac_current: f64,
    pub phase_count: u8,
    pub l1_voltage: f64,
    pub l2_voltage: f64,
    pub l3_voltage: f64,
    pub l1_current: f64,
    pub l2_current: f64,
    pub l3_current: f64,
    pub l1_power: f64,
    pub l2_power: f64,
    pub l3_power: f64,
    pub status: u32,
    pub charging_time: i64,
    pub position: u8,
    pub enable_display: u8,
    pub auto_start: u8,
    pub model: String,
}

pub struct EvCharger {
    pub(crate) values: Mutex<EvChargerValues>,
    pub(crate) commands_tx: mpsc::UnboundedSender<DriverCommand>,
}

#[zbus::interface(name = "com.victronenergy.evcharger")]
impl EvCharger {
    #[zbus(property)]
    fn device_instance(&self) -> u32 {
        self.values.lock().unwrap().device_instance
    }
    #[zbus(property)]
    fn product_name(&self) -> String {
        self.values.lock().unwrap().product_name.clone()
    }
    #[zbus(property)]
    fn firmware_version(&self) -> String {
        self.values.lock().unwrap().firmware_version.clone()
    }
    #[zbus(property)]
    fn product_id(&self) -> u32 {
        self.values.lock().unwrap().product_id
    }
    #[zbus(property)]
    fn connected(&self) -> u8 {
        self.values.lock().unwrap().connected
    }
    #[zbus(property)]
    fn serial(&self) -> String {
        self.values.lock().unwrap().serial.clone()
    }
    #[zbus(property)]
    fn mode(&self) -> u8 {
        self.values.lock().unwrap().mode
    }
    #[zbus(property)]
    fn start_stop(&self) -> u8 {
        self.values.lock().unwrap().start_stop
    }
    #[zbus(property)]
    fn set_current(&self) -> f64 {
        self.values.lock().unwrap().set_current
    }
    #[zbus(property)]
    fn max_current(&self) -> f64 {
        self.values.lock().unwrap().max_current
    }
    #[zbus(property)]
    fn current(&self) -> f64 {
        self.values.lock().unwrap().current
    }
    #[zbus(property)]
    fn ac_power(&self) -> f64 {
        self.values.lock().unwrap().ac_power
    }
    #[zbus(property)]
    fn ac_energy_forward(&self) -> f64 {
        self.values.lock().unwrap().ac_energy_forward
    }
    #[zbus(property)]
    fn ac_energy_total(&self) -> f64 {
        self.values.lock().unwrap().ac_energy_forward
    }
    #[zbus(property)]
    fn ac_current(&self) -> f64 {
        self.values.lock().unwrap().ac_current
    }
    #[zbus(property)]
    fn ac_phase_count(&self) -> u8 {
        self.values.lock().unwrap().phase_count
    }
    #[zbus(property)]
    fn ac_l1_voltage(&self) -> f64 {
        self.values.lock().unwrap().l1_voltage
    }
    #[zbus(property)]
    fn ac_l2_voltage(&self) -> f64 {
        self.values.lock().unwrap().l2_voltage
    }
    #[zbus(property)]
    fn ac_l3_voltage(&self) -> f64 {
        self.values.lock().unwrap().l3_voltage
    }
    #[zbus(property)]
    fn ac_l1_current(&self) -> f64 {
        self.values.lock().unwrap().l1_current
    }
    #[zbus(property)]
    fn ac_l2_current(&self) -> f64 {
        self.values.lock().unwrap().l2_current
    }
    #[zbus(property)]
    fn ac_l3_current(&self) -> f64 {
        self.values.lock().unwrap().l3_current
    }
    #[zbus(property)]
    fn ac_l1_power(&self) -> f64 {
        self.values.lock().unwrap().l1_power
    }
    #[zbus(property)]
    fn ac_l2_power(&self) -> f64 {
        self.values.lock().unwrap().l2_power
    }
    #[zbus(property)]
    fn ac_l3_power(&self) -> f64 {
        self.values.lock().unwrap().l3_power
    }
    #[zbus(property)]
    fn status(&self) -> u32 {
        self.values.lock().unwrap().status
    }
    #[zbus(property)]
    fn charging_time(&self) -> i64 {
        self.values.lock().unwrap().charging_time
    }
    #[zbus(property)]
    fn position(&self) -> u8 {
        self.values.lock().unwrap().position
    }
    #[zbus(property)]
    fn enable_display(&self) -> u8 {
        self.values.lock().unwrap().enable_display
    }
    #[zbus(property)]
    fn auto_start(&self) -> u8 {
        self.values.lock().unwrap().auto_start
    }
    #[zbus(property)]
    fn model(&self) -> String {
        self.values.lock().unwrap().model.clone()
    }

    #[zbus(property)]
    fn set_mode(&self, mode: u8) -> zbus::Result<()> {
        self.commands_tx
            .send(DriverCommand::SetMode(mode))
            .map_err(|_| zbus::Error::Failure("Failed to enqueue SetMode".into()))
    }

    #[zbus(property)]
    fn set_start_stop(&self, v: u8) -> zbus::Result<()> {
        self.commands_tx
            .send(DriverCommand::SetStartStop(v))
            .map_err(|_| zbus::Error::Failure("Failed to enqueue SetStartStop".into()))
    }

    #[zbus(property)]
    fn set_set_current(&self, amps: f64) -> zbus::Result<()> {
        self.commands_tx
            .send(DriverCommand::SetCurrent(amps as f32))
            .map_err(|_| zbus::Error::Failure("Failed to enqueue SetCurrent".into()))
    }

    #[zbus(property)]
    fn set_ac_phase_count(&self, phases: u8) -> zbus::Result<()> {
        self.commands_tx
            .send(DriverCommand::SetPhases(phases))
            .map_err(|_| zbus::Error::Failure("Failed to enqueue SetPhases".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::DriverCommand;

    fn build_test_values() -> EvChargerValues {
        EvChargerValues {
            device_instance: 42,
            product_name: "Test Charger".to_string(),
            firmware_version: "1.2.3".to_string(),
            serial: "ABC123".to_string(),
            product_id: 0xC024,
            connected: 1,
            mode: 2,
            start_stop: 1,
            set_current: 16.0,
            max_current: 32.0,
            current: 15.5,
            ac_power: 3500.0,
            ac_energy_forward: 12.34,
            ac_current: 10.0,
            phase_count: 3,
            l1_voltage: 230.0,
            l2_voltage: 231.0,
            l3_voltage: 229.0,
            l1_current: 5.1,
            l2_current: 5.2,
            l3_current: 5.3,
            l1_power: 1200.0,
            l2_power: 1150.0,
            l3_power: 1150.0,
            status: 2,
            charging_time: 3600,
            position: 1,
            enable_display: 1,
            auto_start: 0,
            model: "AC22NS".to_string(),
        }
    }

    #[test]
    fn getters_identity_values() {
        let (tx, _rx) = mpsc::unbounded_channel::<DriverCommand>();
        let ev = EvCharger {
            values: Mutex::new(build_test_values()),
            commands_tx: tx,
        };

        assert_eq!(ev.device_instance(), 42);
        assert_eq!(ev.product_name(), "Test Charger");
        assert_eq!(ev.firmware_version(), "1.2.3");
        assert_eq!(ev.serial(), "ABC123");
        assert_eq!(ev.product_id(), 0xC024);
        assert_eq!(ev.model(), "AC22NS");
    }

    #[test]
    fn getters_electrical_basic() {
        let (tx, _rx) = mpsc::unbounded_channel::<DriverCommand>();
        let ev = EvCharger {
            values: Mutex::new(build_test_values()),
            commands_tx: tx,
        };

        assert_eq!(ev.connected(), 1);
        assert_eq!(ev.mode(), 2);
        assert_eq!(ev.start_stop(), 1);
        assert!((ev.set_current() - 16.0).abs() < f64::EPSILON);
        assert!((ev.max_current() - 32.0).abs() < f64::EPSILON);
        assert!((ev.current() - 15.5).abs() < f64::EPSILON);
    }

    #[test]
    fn getters_electrical_voltages_and_currents() {
        let (tx, _rx) = mpsc::unbounded_channel::<DriverCommand>();
        let ev = EvCharger {
            values: Mutex::new(build_test_values()),
            commands_tx: tx,
        };

        assert_eq!(ev.ac_phase_count(), 3);
        assert!((ev.ac_l1_voltage() - 230.0).abs() < f64::EPSILON);
        assert!((ev.ac_l2_voltage() - 231.0).abs() < f64::EPSILON);
        assert!((ev.ac_l3_voltage() - 229.0).abs() < f64::EPSILON);
        assert!((ev.ac_l1_current() - 5.1).abs() < f64::EPSILON);
        assert!((ev.ac_l2_current() - 5.2).abs() < f64::EPSILON);
        assert!((ev.ac_l3_current() - 5.3).abs() < f64::EPSILON);
    }

    #[test]
    fn getters_electrical_power_and_energy() {
        let (tx, _rx) = mpsc::unbounded_channel::<DriverCommand>();
        let ev = EvCharger {
            values: Mutex::new(build_test_values()),
            commands_tx: tx,
        };

        assert!((ev.ac_power() - 3500.0).abs() < f64::EPSILON);
        assert!((ev.ac_energy_total() - 12.34).abs() < f64::EPSILON);
        assert!((ev.ac_current() - 10.0).abs() < f64::EPSILON);
        assert!((ev.ac_l1_power() - 1200.0).abs() < f64::EPSILON);
        assert!((ev.ac_l2_power() - 1150.0).abs() < f64::EPSILON);
        assert!((ev.ac_l3_power() - 1150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn getters_status_values() {
        let (tx, _rx) = mpsc::unbounded_channel::<DriverCommand>();
        let ev = EvCharger {
            values: Mutex::new(build_test_values()),
            commands_tx: tx,
        };

        assert_eq!(ev.status(), 2);
        assert_eq!(ev.charging_time(), 3600);
        assert_eq!(ev.position(), 1);
        assert_eq!(ev.enable_display(), 1);
        assert_eq!(ev.auto_start(), 0);
    }

    #[test]
    fn setters_send_commands() {
        let (tx, mut rx) = mpsc::unbounded_channel::<DriverCommand>();
        let ev = EvCharger {
            values: Mutex::new(EvChargerValues::default()),
            commands_tx: tx,
        };

        ev.set_mode(2).unwrap();
        ev.set_start_stop(1).unwrap();
        ev.set_set_current(13.0).unwrap();

        let msg1 = rx.try_recv().unwrap();
        match msg1 {
            DriverCommand::SetMode(v) => assert_eq!(v, 2),
            _ => panic!("expected SetMode"),
        }
        let msg2 = rx.try_recv().unwrap();
        match msg2 {
            DriverCommand::SetStartStop(v) => assert_eq!(v, 1),
            _ => panic!("expected SetStartStop"),
        }
        let msg3 = rx.try_recv().unwrap();
        match msg3 {
            DriverCommand::SetCurrent(v) => assert!((v - 13.0).abs() < f32::EPSILON),
            _ => panic!("expected SetCurrent"),
        }
    }
}
