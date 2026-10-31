# smi-storm-sentinel

[![Rust](https://img.shields.io/badge/rust-2021%20edition-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Prometheus](https://img.shields.io/badge/Prometheus-Port%209114-green.svg)](http://localhost:9114/metrics)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-lightgrey.svg)]()

A zero-allocation, bare-metal Linux Ring -2 SMM (System Management Mode) SMI storm and firmware stolen-time auditor written in pure Rust.

Executes direct x86_64 `RDTSC` assembly instructions against CPU silicon to measure invisible time gaps when BIOS/UEFI firmware abducts the CPU into **Ring -2 SMRAM**, freezing the Linux operating system without kernel awareness. Exposes real-time Prometheus metrics on port `:9114`.

```
===============================================================================
   SMI-STORM-SENTINEL: Ring -2 SMM Firmware Latency & Time-Theft Auditor
===============================================================================
 TSC Frequency:  3.294 GHz  | Cycle Latency: ~0.30 ns | Detection Floor: 100 µs
-------------------------------------------------------------------------------
 SMM Firmware Status:        [OPTIMAL: NO SMM FIRMWARE TIME THEFT DETECTED - CONTINUOUS REALITY]
-------------------------------------------------------------------------------
 [RDTSC HARDWARE LATENCY & TIME-THEFT METRICS]
 Cumulative Stolen Time:          0.00 ns (Total time CPU was abducted into SMRAM)
 Abduction Events Detected:             0 (Hardware gaps exceeding 100 µs threshold)
 Peak Single Execution Freeze:    0.00 ns
 Most Recent Stolen Gap:          0.00 ns
 Active Sampling Loop Jitter:     3.85 µs
-------------------------------------------------------------------------------
 [HARDWARE INTERRUPT PULSE FORENSICS (/proc/interrupts)]
 INTERRUPT TYPE         CUMULATIVE      RATE (/sec)    
 -----------------------------------------------------------------------------
 Non-Maskable (NMI):    1286            +0/s
 TLB Shootdowns (MMU):  14420299        +2120/s
 Rescheduling IPIs:     2029830         +410/s
 Function Call IPIs:    16369070        +1890/s
 Machine Checks (MCE):  0               (Hardware CPU/Bus Parity Errors)
===============================================================================
 Prometheus Exporter: http://0.0.0.0:9114/metrics
 Polling: 1s | PID: 82834 | Press Ctrl+C to shutdown
```

---

## The Eldritch Layer: Ring -2 System Management Mode (SMM)

In modern computer architecture, the operating system kernel (Ring 0) and the hypervisor (Ring -1) believe they have sovereign control over the machine. **They do not.**

Beneath the operating system lies **Ring -2: System Management Mode (SMM)**.

### What Happens During a System Management Interrupt (SMI):
1. **Universal Execution Freeze**: A motherboard thermal sensor, power glitch, or USB legacy emulation asserts the hardware `SMI#` pin.
2. **The CPU Regresses to 1981**: 
   - All CPU cores immediately freeze.
   - The CPU saves its architectural state into a hidden physical memory region called **SMRAM** (locked by the memory controller, invisible to Linux).
   - The CPU drops out of 64-bit protected long mode and **regresses into 16-bit unpaged Real Mode**.
3. **The OS Is Blind**:
   - The Linux kernel is not notified.
   - Interrupts are disabled.
   - Schedulers are halted.
   - If vendor BIOS firmware spends 1,500 microseconds (1.5 ms) inside SMRAM, **the operating system simply drops out of existence**.
4. **The Resume (`RSM`)**:
   - The firmware executes the x86 `RSM` instruction.
   - The CPU restores registers from SMRAM and drops back to Linux.
   - To the kernel, no interrupt occurred. The hardware Time Stamp Counter (TSC) simply skipped forward.

---

## Architectural Mechanics: Proving the Ghost with `RDTSC`

Because the operating system is paralyzed during an SMI, software cannot catch the event using standard kernel APIs.

`smi-storm-sentinel` hunts the ghost using the **Hardware Latency (HWLAT) Cycle Gap Technique**:
- Calibrates the CPU's nominal TSC cycle frequency ($f_{\text{tsc}}$).
- Runs an ultra-tight, lock-free micro-sampling loop executing raw x86 `_rdtsc()` instructions directly against silicon:
  $$\Delta \text{cycles} = \text{TSC}_{\text{now}} - \text{TSC}_{\text{prev}}$$
  $$\Delta t = \frac{\Delta \text{cycles}}{f_{\text{tsc}}}$$
- Under normal CPU execution, $\Delta \text{cycles}$ is 15–30 cycles (~10 nanoseconds).
- If $\Delta t$ suddenly spikes above the detection threshold (e.g. $100\ \mu\text{s}$ or $1\text{ ms}$), the CPU was abducted into Ring -2.
- Correlates against `/proc/interrupts` to verify whether the gap was caused by kernel interrupts (NMI/TLB shootdowns) or **unmasked SMM firmware execution**.

---

## Prometheus Metrics Reference (`:9114/metrics`)

| Metric Name | Type | Description |
|---|---|---|
| `smi_stolen_time_nanoseconds_total` | Counter | Cumulative nanoseconds stolen from the operating system by Ring -2 SMM |
| `smi_abduction_events_total` | Counter | Count of hardware execution freeze gaps exceeding the threshold |
| `smi_max_stolen_time_nanoseconds` | Gauge | Peak single execution freeze caused by hardware firmware |
| `smi_last_gap_nanoseconds` | Gauge | Duration in nanoseconds of the most recently detected stolen time gap |
| `smi_current_jitter_nanoseconds` | Gauge | Peak RDTSC cycle delta jitter measured in the active sampling window |
| `smi_tsc_frequency_hz` | Gauge | Calibrated hardware Time Stamp Counter operating frequency in Hertz |
| `smi_detection_threshold_nanoseconds` | Gauge | Configured threshold for detecting hardware stolen time (e.g. 100 µs) |
| `smi_storm_active` | Gauge | `1` if machine is actively suffering severe SMM time theft (`>200 µs`) |
| `smi_interrupt_nmi_total` | Counter | Cumulative Non-Maskable Interrupts across all CPU cores |
| `smi_interrupt_tlb_shootdowns_total` | Counter | Cross-core MMU TLB cache invalidation shootdown IPIs |
| `smi_interrupt_reschedule_total` | Counter | Inter-processor rescheduling interrupts forcing thread context switches |
| `smi_interrupt_function_call_total` | Counter | Inter-processor cross-core function call synchronization interrupts |
| `smi_interrupt_mce_total` | Counter | Hardware Machine Check Exceptions detected by CPU silicon |

---

## Quickstart

### 1. Build from Source

Requires Rust 1.70+:

```bash
git clone https://github.com/patrick-isles/smi-storm-sentinel.git
cd smi-storm-sentinel
cargo build --release
```

The optimized binary is output to `target/release/smi-storm-sentinel` (~1.2 MB stripped).

### 2. Interactive Terminal Dashboard

```bash
./target/release/smi-storm-sentinel --threshold-us 100
```

### 3. Headless Prometheus Exporter Daemon

```bash
./target/release/smi-storm-sentinel --daemon --port 9114 --threshold-us 100 --interval 0.5
```

Scrape metrics directly:
```bash
curl -s http://127.0.0.1:9114/metrics | grep smi_
```

---

## Production Deployment (systemd)

Install the hardened systemd unit:

```bash
sudo cp target/release/smi-storm-sentinel /usr/local/bin/
sudo cp systemd/smi-storm.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now smi-storm.service
```

Inspect service health:
```bash
systemctl status smi-storm.service
journalctl -u smi-storm.service -f
```

---

## Engineering Design Principles

- **Zero Runtime Allocations**: Micro-sampling loop operates with lock-free atomic registers and zero heap allocation.
- **Direct Silicon Interrogation**: Uses intrinsic `_rdtsc` instructions rather than OS system calls or high-level clock wrappers.
- **Resilient Signal Handling**: Clean shutdown on `SIGINT` and `SIGTERM`.

---

## Author

**Patrick Isles**  
Portland, Oregon  
GitHub: [@patrick-isles](https://github.com/patrick-isles)  
Email: patrick.isles@pcc.edu
