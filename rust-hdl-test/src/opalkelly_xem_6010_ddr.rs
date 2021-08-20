use crate::ok_tools::ok_test_prelude;
use rust_hdl_core::bits::bit_cast;
use rust_hdl_core::prelude::*;
use rust_hdl_ok::ddr_fifo::DDRFIFO;
use rust_hdl_ok::mcb_if::MCBInterface;
use rust_hdl_ok::ok_hi::OpalKellyHostInterface;
use rust_hdl_ok::ok_host::OpalKellyHost;
use rust_hdl_ok::ok_pipe::BTPipeOut;
use rust_hdl_ok::ok_wire::WireIn;
use rust_hdl_ok::pins::{xem_6010_base_clock, xem_6010_leds};
use rust_hdl_ok_frontpanel_sys::{make_u16_buffer, OkError};
use rust_hdl_widgets::prelude::*;
use std::thread::sleep;
use std::time::{Duration, Instant};
use rust_hdl_widgets::fifo_reducer::FIFOReducer;
use rust_hdl_ok::ok_download_ddr::OpalKellyDDRBackedDownloadFIFO;

#[derive(LogicBlock)]
struct OpalKellyDownloadDDRFIFOStressTest {
    mcb: MCBInterface,
    hi: OpalKellyHostInterface,
    ok_host: OpalKellyHost,
    download: OpalKellyDDRBackedDownloadFIFO<0xA0>,
    count_in: DFF<Bits<32>>,
    raw_sys_clock: Signal<In, Clock>,
    strobe: Strobe<48_000_000, 32>,
    will_write: Signal<Local, Bit>,
    reset: WireIn<0x0>,
    enable: WireIn<0x1>,
}

impl Default for OpalKellyDownloadDDRFIFOStressTest {
    fn default() -> Self {
        Self {
            mcb: MCBInterface::xem_6010(),
            hi: OpalKellyHostInterface::xem_6010(),
            ok_host: OpalKellyHost::default(),
            download: Default::default(),
            count_in: Default::default(),
            raw_sys_clock: xem_6010_base_clock(),
            strobe: Strobe::new(4_000_000.0),
            will_write: Default::default(),
            reset: Default::default(),
            enable: Default::default(),
        }
    }
}

impl Logic for OpalKellyDownloadDDRFIFOStressTest {
    #[hdl_gen]
    fn update(&mut self) {
        self.hi.link(&mut self.ok_host.hi);
        self.mcb.link(&mut self.download.mcb);
        self.download.reset.next = self.reset.dataout.val().any();
        self.download.raw_sys_clock.next = self.raw_sys_clock.val();
        self.download.ti_clk.next = self.ok_host.ti_clk.val();
        self.count_in.clk.next = self.ok_host.ti_clk.val();
        self.strobe.clock.next = self.ok_host.ti_clk.val();
        self.download.write_clock.next = self.ok_host.ti_clk.val();
        // Data source - counts on each strobe pulse and writes it to the input FIFO.
        self.will_write.next =
            self.strobe.strobe.val() & !self.download.full.val() & self.enable.dataout.val().any();
        self.count_in.d.next = self.count_in.q.val() + self.will_write.val();
        self.download.data_in.next = self.count_in.q.val();
        self.download.write.next = self.will_write.val();
        self.download.ok1.next = self.ok_host.ok1.val();
        self.reset.ok1.next = self.ok_host.ok1.val();
        self.enable.ok1.next = self.ok_host.ok1.val();
        self.ok_host.ok2.next = self.download.ok2.val();
        self.strobe.enable.next = self.enable.dataout.val().any();
    }
}

#[test]
fn test_opalkelly_xem_6010_ddr_stress() {
    let mut uut = OpalKellyDownloadDDRFIFOStressTest::default();
    uut.hi.link_connect();
    uut.mcb.link_connect();
    uut.raw_sys_clock.connect();
    uut.connect_all();
    crate::ok_tools::synth_obj(uut, "opalkelly_xem_6010_ddr_stress");
}

#[test]
fn test_opalkelly_xem_6010_ddr_stress_runtime() -> Result<(), OkError> {
    let hnd = ok_test_prelude("opalkelly_xem_6010_ddr_stress/top.bit")?;
    hnd.reset_firmware(0);
    sleep(Duration::from_millis(100));
    hnd.set_wire_in(1, 1);
    hnd.update_wire_ins();
    // Read the data in 256*2 = 512 byte blocks
    let mut counter = 0;
    for _ in 0..32 {
        let mut data = vec![0_u8; 1024 * 1024];
        let now = Instant::now();
        hnd.read_from_block_pipe_out(0xA0, 256, &mut data).unwrap();
        let elapsed = (Instant::now() - now).as_micros();
        println!(
            "Download rate is {} mbps",
            (data.len() as f32 * 8.0) / (elapsed as f32 * 1e-6) / 1e6
        );
        let data_shorts = make_u16_buffer(&data);
        let mut data_words = vec![];
        for i in 0..data_shorts.len()/2 {
            let lo_word = data_shorts[2*i] as u32;
            let hi_word = data_shorts[2*i+1] as u32;
            data_words.push((hi_word << 16) | lo_word);
        }
        for (ndx, val) in data_words.iter().enumerate() {
            assert_eq!(((counter as u128) & 0xFFFFFFFF_u128) as u32, *val);
            counter += 1;
        }
    }
    hnd.set_wire_in(1, 0);
    hnd.update_wire_ins();
    hnd.close();
    Ok(())
}
