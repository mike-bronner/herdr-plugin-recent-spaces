use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const INPUTS: [&str; 4] = ["src", "build.rs", "Cargo.toml", "Cargo.lock"];

const GIT_PATHS: [&str; 3] = ["HEAD", "refs", "packed-refs"];

fn main() {
    for input in INPUTS {
        println!("cargo:rerun-if-changed={}", input);
    }
    for path in git_watch_paths() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rustc-env=RECENT_SPACES_COMMIT={}", commit());
    println!("cargo:rustc-env=RECENT_SPACES_BUILT={}", built());
}

fn git_watch_paths() -> Vec<PathBuf> {
    GIT_PATHS
        .iter()
        .filter_map(|name| git(&["rev-parse", "--git-path", name]))
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

fn commit() -> String {
    let Some(short) = git(&["rev-parse", "--short", "HEAD"]) else {
        return "unknown".to_string();
    };
    let mut status = vec!["status", "--porcelain", "--"];
    status.extend(INPUTS);
    match Command::new("git").args(&status).output() {
        Ok(out) if !out.status.success() => format!("{}-unverified", short),
        Err(_) => format!("{}-unverified", short),
        Ok(out) if !String::from_utf8_lossy(&out.stdout).trim().is_empty() => {
            format!("{}-dirty", short)
        }
        Ok(_) => short,
    }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let answer = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match answer.is_empty() {
        true => None,
        false => Some(answer),
    }
}

fn built() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let rest = secs % 86_400;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month,
        day,
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
