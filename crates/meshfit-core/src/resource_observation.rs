use std::{
    collections::{HashMap, HashSet},
    fs,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResourcePeaks {
    pub peak_ram_gb: Option<f64>,
    pub peak_vram_gb: Option<f64>,
}

pub struct ResourceSampler {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<ResourcePeaks>>,
}

impl ResourceSampler {
    pub fn start(root_pid: u32, interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let mut peaks = ResourcePeaks::default();

            while !stop_for_thread.load(Ordering::Relaxed) {
                update_peaks(root_pid, &mut peaks);
                thread::sleep(interval);
            }

            update_peaks(root_pid, &mut peaks);
            peaks
        });

        Self {
            stop,
            handle: Some(handle),
        }
    }

    pub fn finish(mut self) -> ResourcePeaks {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    }
}

impl Drop for ResourceSampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn update_peaks(root_pid: u32, peaks: &mut ResourcePeaks) {
    let snapshot = sample_resource_usage(root_pid);

    if let Some(ram_gb) = snapshot.peak_ram_gb {
        peaks.peak_ram_gb = Some(
            peaks
                .peak_ram_gb
                .map_or(ram_gb, |current| current.max(ram_gb)),
        );
    }

    if let Some(vram_gb) = snapshot.peak_vram_gb {
        peaks.peak_vram_gb = Some(
            peaks
                .peak_vram_gb
                .map_or(vram_gb, |current| current.max(vram_gb)),
        );
    }
}

pub fn sample_resource_usage(root_pid: u32) -> ResourcePeaks {
    #[cfg(target_os = "linux")]
    {
        let processes = read_linux_process_table();
        let descendants = descendant_pids(root_pid, &processes);

        let peak_ram_gb = if descendants.contains(&root_pid) {
            let total_kib = descendants
                .iter()
                .filter_map(|pid| processes.get(pid).and_then(|process| process.rss_kib))
                .sum::<u64>();

            Some(total_kib as f64 / 1024.0 / 1024.0)
        } else {
            None
        };

        let peak_vram_gb = sample_nvidia_vram_mib(&descendants)
            .map(|memory_mib| memory_mib / 1024.0);

        ResourcePeaks {
            peak_ram_gb,
            peak_vram_gb,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = root_pid;
        ResourcePeaks::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessStat {
    ppid: u32,
    rss_kib: Option<u64>,
}

#[cfg(target_os = "linux")]
fn read_linux_process_table() -> HashMap<u32, ProcessStat> {
    let mut processes = HashMap::new();

    let Ok(entries) = fs::read_dir("/proc") else {
        return processes;
    };

    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };

        let status_path = entry.path().join("status");
        let Ok(status) = fs::read_to_string(status_path) else {
            continue;
        };

        if let Some(process) = parse_linux_status(&status) {
            processes.insert(pid, process);
        }
    }

    processes
}

fn parse_linux_status(status: &str) -> Option<ProcessStat> {
    let mut ppid = None;
    let mut rss_kib = None;

    for line in status.lines() {
        if let Some(value) = line.strip_prefix("PPid:") {
            ppid = value.split_whitespace().next()?.parse::<u32>().ok();
        } else if let Some(value) = line.strip_prefix("VmRSS:") {
            rss_kib = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok());
        }
    }

    Some(ProcessStat {
        ppid: ppid?,
        rss_kib,
    })
}

fn descendant_pids(root_pid: u32, processes: &HashMap<u32, ProcessStat>) -> HashSet<u32> {
    let mut descendants = HashSet::new();

    if !processes.contains_key(&root_pid) {
        return descendants;
    }

    descendants.insert(root_pid);

    loop {
        let before = descendants.len();

        for (pid, process) in processes {
            if descendants.contains(&process.ppid) {
                descendants.insert(*pid);
            }
        }

        if descendants.len() == before {
            break;
        }
    }

    descendants
}

#[cfg(target_os = "linux")]
fn sample_nvidia_vram_mib(pids: &HashSet<u32>) -> Option<f64> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(parse_nvidia_compute_apps_mib(&stdout, pids))
}

fn parse_nvidia_compute_apps_mib(raw: &str, pids: &HashSet<u32>) -> f64 {
    raw.lines()
        .filter_map(|line| {
            let (pid, memory) = line.split_once(',')?;
            let pid = pid.trim().parse::<u32>().ok()?;
            if !pids.contains(&pid) {
                return None;
            }

            memory.trim().parse::<f64>().ok()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_status_fields() {
        let process = parse_linux_status(
            "Name:\tpython\nState:\tS (sleeping)\nPPid:\t42\nVmRSS:\t2048 kB\n",
        )
        .unwrap();

        assert_eq!(
            process,
            ProcessStat {
                ppid: 42,
                rss_kib: Some(2048),
            }
        );
    }

    #[test]
    fn follows_dynamic_process_tree() {
        let processes = HashMap::from([
            (
                10,
                ProcessStat {
                    ppid: 1,
                    rss_kib: Some(100),
                },
            ),
            (
                11,
                ProcessStat {
                    ppid: 10,
                    rss_kib: Some(200),
                },
            ),
            (
                12,
                ProcessStat {
                    ppid: 11,
                    rss_kib: Some(300),
                },
            ),
            (
                99,
                ProcessStat {
                    ppid: 1,
                    rss_kib: Some(999),
                },
            ),
        ]);

        assert_eq!(
            descendant_pids(10, &processes),
            HashSet::from([10, 11, 12])
        );
    }

    #[test]
    fn sums_only_selected_process_tree_vram() {
        let pids = HashSet::from([10, 11]);
        let raw = "10, 1024\n11, 512\n99, 4096\n";

        assert_eq!(parse_nvidia_compute_apps_mib(raw, &pids), 1536.0);
    }

    #[test]
    fn missing_root_produces_no_process_tree() {
        let processes = HashMap::from([(
            11,
            ProcessStat {
                ppid: 10,
                rss_kib: Some(200),
            },
        )]);

        assert!(descendant_pids(10, &processes).is_empty());
    }
}
