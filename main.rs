use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

struct TargetDir {
    path: PathBuf,
    size_bytes: u64,
    last_modified_days: u64,
}

fn main() {
    println!("\x1b[1;36mScanning for Rust target/ directories across projects...\x1b[0m\n");

    let home_dir = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut targets = Vec::new();
    
    scan_workspace(&home_dir, &mut targets);

    if targets.is_empty() {
        println!("\x1b[1;32m✔ No target directories found.\x1b[0m");
        return;
    }

    // Partition by moving ownership completely, avoiding any borrow checker conflicts
    let (stale_targets, active_targets): (Vec<TargetDir>, Vec<TargetDir>) = 
        targets.into_iter().partition(|t| t.last_modified_days > 14);

    let total_bytes: u64 = active_targets.iter().map(|t| t.size_bytes).sum::<u64>() 
        + stale_targets.iter().map(|t| t.size_bytes).sum::<u64>();
    let stale_bytes: u64 = stale_targets.iter().map(|t| t.size_bytes).sum();

    // Standard binary Gibibyte unit allocation
    let gib = 1024.0 * 1024.0 * 1024.0;

    println!("\x1b[1mDisk Space Breakdown:\x1b[0m");
    println!(" Total Rust build caches found: \x1b[33m{:.2} GiB\x1b[0m across {} projects", total_bytes as f64 / gib, active_targets.len() + stale_targets.len());
    println!(" Stale build caches (>14 days old): \x1b[31m{ {:.2} GiB\x1b[0m across {} projects\n", stale_bytes as f64 / gib, stale_targets.len());

    if !stale_targets.is_empty() {
        println!("\x1b[1;33mTop disk consumers (>14 days inactive):\x1b[0m");
        for target in stale_targets.iter().take(10) {
            println!(
                "  \x1b[31m[{:.2} GiB]\x1b[0m {} \x1b[90m({} days idle)\x1b[0m",
                target.size_bytes as f64 / gib,
                target.path.display(),
                target.last_modified_days
            );
        }

        println!("\n\x1b[1mInitiating absolute purge sequence...\x1b[0m");
        let mut reclaimed = 0u64;
        for target in stale_targets {
            if fs::remove_dir_all(&target.path).is_ok() {
                println!("  \x1b[32m✔ Purged:\x1b[0m {}", target.path.display());
                reclaimed += target.size_bytes;
            } else {
                println!("  \x1b[31m✖ Failed to remove:\x1b[0m {}", target.path.display());
            }
        }
        println!("\n\x1b[1;32m✔ Successfully reclaimed {:.2} GiB of disk space!\x1b[0m", reclaimed as f64 / gib);
    }
}

fn scan_workspace(base_dir: &Path, targets: &mut Vec<TargetDir>) {
    // WalkDir handles cross-platform directory iterations and filters hidden folders instantly
    let mut it = WalkDir::new(base_dir)
        .follow_links(false) // Prevents circular loops / infinite cycles safely
        .into_iter();

    loop {
        let entry = match it.next() {
            None => break,
            Some(Err(_)) => continue, // Skip unreadable directories/system files gracefully
            Some(Ok(entry)) => entry,
        };

        let path = entry.path();
        let file_name = entry.file_name().to_str().unwrap_or("");

        // Skip massive non-project configuration roots to maximize scanning efficiency
        if entry.file_type().is_dir() {
            if file_name.starts_with('.') || file_name == "node_modules" || file_name == "Library" || file_name == "Applications" {
                it.skip_current_dir(); // Skips recursive parsing inside these roots entirely
                continue;
            }

            if file_name == "target" && is_rust_target_dir(path) {
                let size_bytes = dir_size(path);
                let last_modified_days = days_since_modified(path);
                
                targets.push(TargetDir {
                    path: path.to_path_buf(),
                    size_bytes,
                    last_modified_days,
                });
                
                it.skip_current_dir(); // No need to scan inside target/ once identified
            }
        }
    }
}

fn is_rust_target_dir(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        return parent.join("Cargo.toml").exists();
    }
    false
}

fn dir_size(path: &Path) -> u64 {
    // Fast file sizing using WalkDir framework abstractions
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn days_since_modified(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|mod_time| {
            SystemTime::now()
                .duration_since(mod_time)
                .unwrap_or(Duration::from_secs(0))
                .as_secs() / 86400
        })
        .unwrap_or(0)
}
