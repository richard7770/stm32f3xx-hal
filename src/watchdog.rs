use crate::hal::watchdog::{Watchdog, WatchdogEnable};

use crate::stm32::{DBGMCU, IWDG};
use crate::time::MilliSeconds;

const LSI_KHZ: u32 = 40;
const MAX_PR: u8 = 8;
const MAX_RL: u16 = 0x1000;
const KR_ACCESS: u16 = 0x5555;
const KR_RELOAD: u16 = 0xAAAA;
const KR_START: u16 = 0xCCCC;

pub struct IndependentWatchDog {
    iwdg: IWDG,
}

impl IndependentWatchDog {
    pub fn new(iwdg: IWDG) -> Self {
        IndependentWatchDog { iwdg }
    }

    pub fn stop_on_debug(&self, dbg: &DBGMCU, stop: bool) {
        dbg.apb1_fz.modify(|_, w| w.dbg_iwdg_stop().bit(stop));
    }

    fn setup(&self, timeout_ms: u32) {
        let mut pr = 0;
        while pr < MAX_PR && Self::timeout_period(pr, MAX_RL) < timeout_ms {
            pr += 1;
        }

        let max_period = Self::timeout_period(pr, MAX_RL);
        let max_rl = u32::from(MAX_RL);
        let rl = (timeout_ms * max_rl / max_period).min(max_rl) as u16;

        self.access_registers(|iwdg| {
            iwdg.pr.modify(|_, w| w.pr().bits(pr));
            iwdg.rlr.modify(|_, w| w.rl().bits(rl));
        });
    }

    fn is_pr_updating(&self) -> bool {
        self.iwdg.sr.read().pvu().bit()
    }

    /// Returns the interval in ms
    pub fn interval(&self) -> MilliSeconds {
        while self.is_pr_updating() {}

        let pr = self.iwdg.pr.read().pr().bits();
        let rl = self.iwdg.rlr.read().rl().bits();
        let ms = Self::timeout_period(pr, rl);
        MilliSeconds(ms)
    }

    /// pr: Prescaler divider bits, rl: reload value
    ///
    /// Returns ms
    fn timeout_period(pr: u8, rl: u16) -> u32 {
        let divider: u32 = match pr {
            0b000 => 4,
            0b001 => 8,
            0b010 => 16,
            0b011 => 32,
            0b100 => 64,
            0b101 => 128,
            0b110 => 256,
            0b111 => 256,
            _ => panic!("Invalid IWDG prescaler divider"),
        };
        (u32::from(rl) + 1) * divider / LSI_KHZ
    }

    fn access_registers<A, F: FnMut(&IWDG) -> A>(&self, mut f: F) -> A {
        // Unprotect write access to registers
        self.iwdg.kr.write(|w| unsafe { w.key().bits(KR_ACCESS) });
        let a = f(&self.iwdg);

        // Protect again
        self.iwdg.kr.write(|w| unsafe { w.key().bits(KR_RELOAD) });
        a
    }
}

impl WatchdogEnable for IndependentWatchDog {
    type Time = MilliSeconds;

    fn start<T: Into<Self::Time>>(&mut self, period: T) {
        self.setup(period.into().0);

        self.iwdg.kr.write(|w| unsafe { w.key().bits(KR_START) });
    }
}

impl Watchdog for IndependentWatchDog {
    fn feed(&mut self) {
        self.iwdg.kr.write(|w| unsafe { w.key().bits(KR_RELOAD) });
    }
}
