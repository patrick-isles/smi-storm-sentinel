//! Linux Low-Level Hardware Interrupt & Subsystem Scanner
//!
//! Interrogates `/proc/interrupts` to audit Non-Maskable Interrupts (NMI),
//! TLB shootdowns, Rescheduling IPIs, and Machine Check Exceptions (MCE).

use std::fs;

#[derive(Debug, Clone, Default)]
pub struct InterruptStats {
    pub nmi_total: u64,
    pub loc_total: u64,
    pub res_total: u64,
    pub cal_total: u64,
    pub tlb_total: u64,
    pub trm_total: u64,
    pub mce_total: u64,
    pub err_total: u64,
    pub mis_total: u64,
}

pub struct InterruptScanner;

impl InterruptScanner {
    /// Reads and parses `/proc/interrupts` hardware interrupt counters.
    pub fn read_interrupts() -> InterruptStats {
        let content = fs::read_to_string("/proc/interrupts").unwrap_or_default();
        let mut stats = InterruptStats::default();

        for line in content.lines() {
            let line_trimmed = line.trim();
            if let Some((label, values_str)) = line_trimmed.split_once(':') {
                let tag = label.trim();
                let sum = Self::sum_row(values_str);

                match tag {
                    "NMI" => stats.nmi_total = sum,
                    "LOC" => stats.loc_total = sum,
                    "RES" => stats.res_total = sum,
                    "CAL" => stats.cal_total = sum,
                    "TLB" => stats.tlb_total = sum,
                    "TRM" => stats.trm_total = sum,
                    "MCE" => stats.mce_total = sum,
                    "ERR" => stats.err_total = sum,
                    "MIS" => stats.mis_total = sum,
                    _ => {}
                }
            }
        }

        stats
    }

    fn sum_row(values_str: &str) -> u64 {
        let mut total = 0u64;
        for token in values_str.split_whitespace() {
            // Numbers are CPU counts; stop if we hit non-numeric description
            if let Ok(count) = token.parse::<u64>() {
                total = total.saturating_add(count);
            } else {
                break;
            }
        }
        total
    }
}
