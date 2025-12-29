use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use log::error;

pub const RUN_REQUEST_DIR: &str = "/sdcard/cardputer";
pub const RUN_REQUEST_PATH: &str = "/sdcard/cardputer/run_py.txt";
pub const PYTHON_RUNNER_BIN_PATH: &str = "/sdcard/cardputer/python_runner.bin";

pub const LEGACY_RUN_REQUEST_DIR: &str = "/sdcard/.cardputer";
pub const LEGACY_RUN_REQUEST_PATH: &str = "/sdcard/.cardputer/run_py.txt";
pub const LEGACY_PYTHON_RUNNER_BIN_PATH: &str = "/sdcard/.cardputer/python_runner.bin";

pub fn write_run_request(path: &Path) -> std::io::Result<()> {
    let contents = path.to_string_lossy();
    if let Err(err) = write_run_request_at(RUN_REQUEST_DIR, RUN_REQUEST_PATH, &contents) {
        error!(
            "run request write failed at {} for {}: {}",
            RUN_REQUEST_PATH, contents, err
        );
        if let Err(legacy_err) =
            write_run_request_at(LEGACY_RUN_REQUEST_DIR, LEGACY_RUN_REQUEST_PATH, &contents)
        {
            error!(
                "legacy run request write failed at {} for {}: {}",
                LEGACY_RUN_REQUEST_PATH, contents, legacy_err
            );
            return Err(err);
        }
    }
    Ok(())
}

pub fn read_run_request() -> std::io::Result<Option<PathBuf>> {
    match read_run_request_at(RUN_REQUEST_PATH) {
        Ok(Some(path)) => return Ok(Some(path)),
        Ok(None) => {}
        Err(err) => error!("run request read failed at {}: {}", RUN_REQUEST_PATH, err),
    }
    read_run_request_at(LEGACY_RUN_REQUEST_PATH)
}

pub fn clear_run_request() -> std::io::Result<()> {
    let mut first_err = None;
    for path in [RUN_REQUEST_PATH, LEGACY_RUN_REQUEST_PATH] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                error!("run request cleanup failed at {}: {}", path, err);
                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
        }
    }
    match first_err {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn write_run_request_at(dir: &str, path: &str, contents: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut file = fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn read_run_request_at(path: &str) -> std::io::Result<Option<PathBuf>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let trimmed = contents.lines().next().unwrap_or("").trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(trimmed)))
}
