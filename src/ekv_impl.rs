#![cfg(feature = "ekv")]

use ekv::flash::{Flash, PageID};
use embedded_hal_async::spi::SpiDevice;

use crate::{Error, Memory, driver};

const _: () = {
    assert!(driver::SECTOR_SIZE == ekv::config::PAGE_SIZE as u32,);
};

impl<SPI: SpiDevice> Flash for Memory<SPI> {
    type Error = Error<SPI>;

    fn page_count(&self) -> usize {
        (driver::END_ADDR / driver::SECTOR_SIZE) as usize
    }

    async fn erase(&mut self, page_id: PageID) -> Result<(), Self::Error> {
        let addr = as_addr(page_id, 0);
        self.erase_sector(addr).await
    }

    async fn read(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &mut [u8],
    ) -> Result<(), Self::Error> {
        let addr = as_addr(page_id, offset);
        self.read(addr, data).await
    }

    async fn write(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        let init_mem_addr = as_addr(page_id, offset);

        let mut start_buf_addr = 0;
        let mut start_mem_addr = init_mem_addr;

        while start_buf_addr < data.len() {
            let bytes_to_write = ((next_multiple(start_mem_addr + 1, driver::PAGE_SIZE)
                - start_mem_addr) as usize)
                .min(data.len() - start_buf_addr);

            self.write_page_partial(
                start_mem_addr,
                &data[start_buf_addr..start_buf_addr + bytes_to_write],
            )
            .await?;

            start_mem_addr += bytes_to_write as u32;
            start_buf_addr += bytes_to_write;
        }

        Ok(())
    }
}

fn as_addr(page_id: PageID, offset: usize) -> u32 {
    page_id.index() as u32 * driver::SECTOR_SIZE + offset as u32
}

fn next_multiple(lhs: u32, rhs: u32) -> u32 {
    lhs.div_ceil(rhs) * rhs
}
