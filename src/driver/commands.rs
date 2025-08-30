use super::{AlfenDriver, DriverCommand};

impl AlfenDriver {
    pub(crate) async fn handle_command(&mut self, cmd: DriverCommand) {
        match cmd {
            DriverCommand::SetMode(m) => self.set_mode(m).await,
            DriverCommand::SetStartStop(v) => self.set_start_stop(v).await,
            DriverCommand::SetCurrent(a) => self.set_intended_current(a).await,
            DriverCommand::SetPhases(p) => self.set_phases(p).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn handle_command_dispatches() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut d = AlfenDriver::new(rx, tx).await.unwrap();
        assert_eq!(d.current_mode_code(), 0);
        d.handle_command(DriverCommand::SetMode(1)).await;
        assert_eq!(d.current_mode_code(), 1);

        d.handle_command(DriverCommand::SetStartStop(1)).await;
        assert_eq!(d.start_stop_code(), 1);

        d.handle_command(DriverCommand::SetCurrent(5.5)).await;
        assert!((d.get_intended_set_current() - 5.5).abs() < f32::EPSILON);
    }
}
