use crate::error::Result;
use std::any::Any;

#[async_trait::async_trait]
pub trait ModbusLike: Send {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Optional connection status. Default: unknown (None).
    fn connection_status(&self) -> Option<bool> {
        None
    }
    async fn read_holding_registers(
        &mut self,
        slave_id: u8,
        address: u16,
        count: u16,
    ) -> Result<Vec<u16>>;

    async fn write_multiple_registers(
        &mut self,
        slave_id: u8,
        address: u16,
        values: &[u16],
    ) -> Result<()>;
}
