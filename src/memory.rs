use core::fmt;

use bitflags::bitflags;
use embedded_hal::{
    delay::DelayNs,
    spi::{Operation, SpiDevice},
};
use sub_array::SubArray;
use thiserror::Error;

use crate::util::{debug, trace};

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

pub struct Memory<SPI: SpiDevice, DELAY: DelayNs> {
    spi: SPI,
    delay: DELAY,
}

impl<SPI: SpiDevice, DELAY: DelayNs> Memory<SPI, DELAY> {
    pub fn new(spi: SPI, delay: DELAY) -> Self {
        Self { spi, delay }
    }

    pub fn reset(&mut self) -> Result<(), Error<SPI>> {
        debug!("reset init");

        let cmd = [0x66, 0x99];

        self.spi.write(&cmd).map_err(Error::IO)?;
        self.delay.delay_us(30 + 5);

        debug!("reset done");
        Ok(())
    }

    pub fn get_jedec_id(&mut self) -> Result<[u8; 3], Error<SPI>> {
        debug!("read JEDEC ID init");

        let cmd = [0x9F];
        let mut data = [0u8; 3];

        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut data)])
            .map_err(Error::IO)?;

        debug!("read JEDEC ID done: {data:02X?}");
        Ok(data)
    }

    pub fn check_jedec_id(&mut self) -> Result<(), Error<SPI>> {
        debug!("check JEDEC ID init");

        let jedec_id = self.get_jedec_id()?;

        if jedec_id == JEDEC_ID {
            debug!("check JEDEC ID done");
            Ok(())
        } else {
            Err(Error::WrongJedecId(jedec_id))
        }
    }

    pub fn read(&mut self, addr: Addr, data: &mut [u8]) -> Result<(), Error<SPI>> {
        debug!("read data init: {} bytes at {}", data.len(), addr);

        let mut cmd = [0u8; 4];
        cmd[0] = 0x0B;
        Self::addr_write_bytes(addr, cmd.sub_array_mut(1), None)?;

        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(data)])
            .map_err(Error::IO)?;

        debug!("read data done: {} bytes at {}", data.len(), addr);
        Ok(())
    }

    pub fn sector_erase(&mut self, addr: Addr) -> Result<(), Error<SPI>> {
        debug!("sector erase init: at {}", addr);

        self.write_enable()?;

        let mut cmd = [0u8; 4];
        cmd[0] = 0x20;
        Self::addr_write_bytes(addr, cmd.sub_array_mut(1), Some(SECTOR_SIZE))?;

        self.spi.write(&cmd).map_err(Error::IO)?;
        self.block_until_ready()?;

        debug!("sector erase done: at {}", addr);
        Ok(())
    }

    pub fn chip_erase(&mut self) -> Result<(), Error<SPI>> {
        debug!("chip erase init");

        self.write_enable()?;

        let cmd = [0x60];
        self.spi.write(&cmd).map_err(Error::IO)?;
        self.block_until_ready()?;

        debug!("chip erase done");
        Ok(())
    }

    pub fn page_write(
        &mut self,
        addr: Addr,
        data: &[u8; PAGE_SIZE as usize],
    ) -> Result<(), Error<SPI>> {
        debug!("write data init: at {}", addr);

        self.write_enable()?;

        let mut cmd = [0u8; 4];
        cmd[0] = 0x02;
        Self::addr_write_bytes(addr, cmd.sub_array_mut(1), Some(PAGE_SIZE))?;

        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Write(data)])
            .map_err(Error::IO)?;
        self.block_until_ready()?;

        debug!("write data done: at {}", addr);
        Ok(())
    }

    pub fn get_status_1(&mut self) -> Result<Status1, Error<SPI>> {
        trace!("get status init");

        let cmd = [0x05];
        let mut data = [0; 1];
        self.spi
            .transaction(&mut [Operation::Write(&cmd), Operation::Read(&mut data)])
            .map_err(Error::IO)?;

        let status = Status1::from_bits(data[0]).expect("unreachable");

        trace!("get status done: {status:?}");
        Ok(status)
    }

    fn block_until_ready(&mut self) -> Result<(), Error<SPI>> {
        debug!("blocking until ready init");

        // XXX: let's hope this loop will ever finish
        loop {
            let status = self.get_status_1()?;
            if !status.contains(Status1::IsBusy) {
                break;
            }
        }

        debug!("blocking until ready done");
        Ok(())
    }

    fn write_enable(&mut self) -> Result<(), Error<SPI>> {
        trace!("write enable init");
        let cmd = [0x06];

        self.spi.write(&cmd).map_err(Error::IO)?;

        trace!("write enable done");
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
