use core::fmt;

use bitflags::bitflags;
use embassy_time::{Duration, Timer};
use embedded_hal_async::spi::{Operation, SpiDevice};
use sub_array::SubArray;
use thiserror::Error;

use crate::util::trace;

pub type Addr = u32;

pub const JEDEC_ID: [u8; 3] = [0xEF, 0x40, 0x16];

pub const SECTOR_SIZE: Addr = 4096; // 4K
pub const PAGE_SIZE: Addr = 256;
pub const END_ADDR: Addr = 4194304; // 4M

#[derive(Error)]
pub enum Error<SPI: SpiDevice> {
    #[error("failed to perform IO operation on the memory chip")]
    IO(SPI::Error),

    #[error("wrong JEDEC ID: expected: {JEDEC_ID:02X?}, got: {0:02X?}")]
    WrongJedecId([u8; 3]),

    #[error("wrong alignment: address {addr:X} is not a multiple of {align:X}")]
    WrongAlignment { addr: Addr, align: Addr },

    #[error("address should be less than {END_ADDR:X}, got: {addr:X}")]
    AddressOverflow { addr: Addr },

    #[error("device were in a busy state for too long")]
    UnexpectedBusy,
}

impl<SPI: SpiDevice> fmt::Debug for Error<SPI> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::IO(arg0) => f.debug_tuple("IO").field(arg0).finish(),
            Error::WrongJedecId(arg0) => f.debug_tuple("WrongJedecId").field(arg0).finish(),
            Error::WrongAlignment { addr, align } => f
                .debug_struct("WrongAlignment")
                .field("addr", addr)
                .field("align", align)
                .finish(),
            Error::AddressOverflow { addr } => f
                .debug_struct("AddressOverflow")
                .field("addr", addr)
                .finish(),
            Error::UnexpectedBusy => f.debug_struct("UnexpectedBusy").finish(),
        }
    }
}

bitflags! {
    /// The Status-1 reguster
    ///
    /// NOTE:
    /// This struct is incomplete. Currenly it only include those values,
    /// that is needed for the implementation.
    #[derive(Debug, Clone, Copy)]
    pub struct Status1: u8 {
        const IsBusy = 1 << 0;
        const _ = !0;
    }
}

pub struct Memory<SPI: SpiDevice> {
    spi: SPI,
}

impl<SPI: SpiDevice> Memory<SPI> {
    pub fn new(spi: SPI) -> Self {
        Self { spi }
    }

    pub async fn reset(&mut self) -> Result<(), Error<SPI>> {
        trace!("reset init");

        let cmd = [0x66, 0x99];

        self.spi.write(&cmd).await.map_err(Error::IO)?;
        self.block_until_ready(Duration::from_micros(30 + 5))
            .await?;
        Timer::after_micros(30 + 5).await;

        trace!("reset done");
        Ok(())
    }

    pub async fn get_jedec_id(&mut self) -> Result<[u8; 3], Error<SPI>> {
        trace!("read JEDEC ID init");

        let cmd = [0x9F];
        let mut data = [0u8; 3];

        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut data)])
            .await
            .map_err(Error::IO)?;

        trace!("read JEDEC ID done: {data:02X?}");
        Ok(data)
    }

    pub async fn check_jedec_id(&mut self) -> Result<(), Error<SPI>> {
        trace!("check JEDEC ID init");

        let jedec_id = self.get_jedec_id().await?;

        if jedec_id == JEDEC_ID {
            trace!("check JEDEC ID done");
            Ok(())
        } else {
            Err(Error::WrongJedecId(jedec_id))
        }
    }

    pub async fn read(&mut self, addr: Addr, data: &mut [u8]) -> Result<(), Error<SPI>> {
        trace!("read data init: {} bytes at {}", data.len(), addr);

        let mut cmd = [0u8; 4];
        cmd[0] = 0x0B;
        Self::addr_write_bytes(addr, cmd.sub_array_mut(1), None)?;

        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(data)])
            .await
            .map_err(Error::IO)?;

        trace!("read data done: {} bytes at {}", data.len(), addr);
        Ok(())
    }

    pub async fn sector_erase(&mut self, addr: Addr) -> Result<(), Error<SPI>> {
        trace!("sector erase init: at {}", addr);

        self.write_enable().await?;

        let mut cmd = [0u8; 4];
        cmd[0] = 0x20;
        Self::addr_write_bytes(addr, cmd.sub_array_mut(1), Some(SECTOR_SIZE))?;

        self.spi.write(&cmd).await.map_err(Error::IO)?;
        self.block_until_ready(Duration::from_millis(400 + 50))
            .await?;

        trace!("sector erase done: at {}", addr);
        Ok(())
    }

    pub async fn chip_erase(&mut self) -> Result<(), Error<SPI>> {
        trace!("chip erase init");

        self.write_enable().await?;

        let cmd = [0x60];
        self.spi.write(&cmd).await.map_err(Error::IO)?;
        self.block_until_ready(Duration::from_secs(50 + 5)).await?;

        trace!("chip erase done");
        Ok(())
    }

    pub async fn page_write(
        &mut self,
        addr: Addr,
        data: &[u8; PAGE_SIZE as usize],
    ) -> Result<(), Error<SPI>> {
        trace!("write data init: at {}", addr);

        self.write_enable().await?;

        let mut cmd = [0u8; 4];
        cmd[0] = 0x02;
        Self::addr_write_bytes(addr, cmd.sub_array_mut(1), Some(PAGE_SIZE))?;

        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Write(data)])
            .await
            .map_err(Error::IO)?;
        self.block_until_ready(Duration::from_millis(3 + 1)).await?;

        trace!("write data done: at {}", addr);
        Ok(())
    }

    pub async fn get_status_1(&mut self) -> Result<Status1, Error<SPI>> {
        let cmd = [0x05];
        let mut data = [0; 1];

        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut data)])
            .await
            .map_err(Error::IO)?;

        let status = Status1::from_bits(data[0]).expect("unreachable");

        Ok(status)
    }

    async fn write_enable(&mut self) -> Result<(), Error<SPI>> {
        trace!("write enable init");
        let cmd = [0x06];

        self.spi.write(&cmd).await.map_err(Error::IO)?;

        trace!("write enable done");
        Ok(())
    }

    async fn block_until_ready(&mut self, timeout: Duration) -> Result<(), Error<SPI>> {
        trace!("blocking until ready init");

        let poll_delay = timeout / 100;

        embassy_time::with_timeout(timeout, async {
            loop {
                let status = self.get_status_1().await?;

                if !status.contains(Status1::IsBusy) {
                    break Ok(());
                }

                Timer::after(poll_delay).await;
            }
        })
        .await
        .map_err(|_| Error::UnexpectedBusy)??;

        trace!("blocking until ready done");
        Ok(())
    }

    fn addr_write_bytes(
        addr: Addr,
        bytes: &mut [u8; 3],
        align: Option<Addr>,
    ) -> Result<(), Error<SPI>> {
        if addr >= END_ADDR {
            return Err(Error::AddressOverflow { addr });
        }

        if let Some(align) = align
            && !addr.is_multiple_of(align)
        {
            return Err(Error::WrongAlignment { addr, align });
        }

        let addr_bytes_full = addr.to_be_bytes();
        let (zero_bytes, addr_bytes) = addr_bytes_full.split_at(addr_bytes_full.len() - 3);
        assert!(
            zero_bytes.iter().all(|zero| *zero == 0),
            "addr {addr} won't fit in 3 bytes"
        );

        bytes.clone_from_slice(addr_bytes);
        Ok(())
    }
}
