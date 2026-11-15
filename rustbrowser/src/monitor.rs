//! Замер памяти и CPU процессов ORVYN (оболочка + WebView2).

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub ram_bytes: u64,
    pub cpu_ticks: u64,
    pub procs: u32,
}

fn ft(v: FILETIME) -> u64 {
    ((v.dwHighDateTime as u64) << 32) | v.dwLowDateTime as u64
}

fn usage(h: HANDLE) -> (u64, u64) {
    unsafe {
        let mut mem = PROCESS_MEMORY_COUNTERS_EX::default();
        mem.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let mut ram = 0u64;
        if GetProcessMemoryInfo(
            h,
            &mut mem as *mut PROCESS_MEMORY_COUNTERS_EX as *mut _,
            mem.cb,
        )
        .is_ok()
        {
            ram = mem.PrivateUsage as u64;
        }
        let (mut c, mut e, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let mut cpu = 0u64;
        if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u).is_ok() {
            cpu = ft(k).saturating_add(ft(u));
        }
        (ram, cpu)
    }
}

/// Суммарные показатели оболочки и переданных процессов движка.
pub fn sample(pids: &[u32]) -> Sample {
    let mut all = Vec::with_capacity(pids.len() + 1);
    all.push(std::process::id());
    for p in pids {
        if !all.contains(p) {
            all.push(*p);
        }
    }

    let mut s = Sample::default();
    for pid in all {
        let Ok(h) = (unsafe {
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
        }) else {
            continue;
        };
        let (ram, cpu) = usage(h);
        unsafe { let _ = CloseHandle(h); }
        if ram > 0 || cpu > 0 {
            s.ram_bytes = s.ram_bytes.saturating_add(ram);
            s.cpu_ticks = s.cpu_ticks.saturating_add(cpu);
            s.procs += 1;
        }
    }
    s
}

/// Процент CPU за интервал (как в диспетчере задач — доля всей машины).
pub fn cpu_percent(prev_ticks: u64, now_ticks: u64, elapsed: std::time::Duration) -> f32 {
    let dt = elapsed.as_nanos() as f64 / 100.0; // тики по 100 нс
    if dt <= 0.0 || now_ticks <= prev_ticks {
        return 0.0;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as f64;
    let pct = (now_ticks - prev_ticks) as f64 / dt * 100.0 / cores;
    ((pct.clamp(0.0, 100.0) * 10.0).round() / 10.0) as f32
}
