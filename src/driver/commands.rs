use super::{AlfenDriver, DriverCommand};

impl AlfenDriver {
    pub(crate) async fn handle_command(&mut self, cmd: DriverCommand) {
        match cmd {
            DriverCommand::SetMode(m) => self.set_mode(m).await,
            DriverCommand::SetStartStop(v) => self.set_start_stop(v).await,
            DriverCommand::SetCurrent(a) => self.set_intended_current(a).await,
        }
    }
}
