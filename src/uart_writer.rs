// use {embedded_hal::blocking::serial::Write, esp32_hal::uart::Uart};

use esp_idf_svc::{
    hal::{task::block_on, uart},
    io::Write,
    sys::EspError,
};

pub struct UartWriter<'d> {
    uart: uart::AsyncUartTxDriver<'d, uart::UartTxDriver<'d>>,
}

impl<'d> UartWriter<'d> {
    pub fn new(uart: uart::AsyncUartTxDriver<'d, uart::UartTxDriver<'d>>) -> Self {
        Self { uart }
    }
}
impl<'d> embedded_hal::blocking::serial::Write<u8> for UartWriter<'d> {
    type Error = EspError;

    fn bwrite_all(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
        embassy_futures::block_on(async {
            self.uart.write(buffer).await.unwrap();
            Ok(())
        })
    }

    fn bflush(&mut self) -> Result<(), Self::Error> {
        embassy_futures::block_on(async {
            self.uart.wait_done().await.unwrap();
            Ok(())
        })
    }
}
