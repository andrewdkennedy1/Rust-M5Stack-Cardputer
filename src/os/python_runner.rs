use std::path::Path;

use crate::keyboard::CardputerKeyboard;
use log::error;

use crate::run_request::{
    write_run_request, LEGACY_PYTHON_RUNNER_BIN_PATH, PYTHON_RUNNER_BIN_PATH,
};
use crate::swapchain::DoubleBuffer;
use crate::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::chainload::{self, FlashError};
use super::ui::show_message_and_wait;

#[derive(Debug)]
enum RunnerLaunchError {
    WriteRequest(std::io::Error),
    RunnerMissing,
    Flash(FlashError),
}

impl RunnerLaunchError {
    fn to_lines(&self) -> Vec<String> {
        match self {
            RunnerLaunchError::WriteRequest(err) => {
                vec![format!("Write failed: {}", err)]
            }
            RunnerLaunchError::RunnerMissing => vec![
                "python_runner.bin not found.".to_string(),
                format!("Place it at {}", PYTHON_RUNNER_BIN_PATH),
                format!("(Legacy path: {})", LEGACY_PYTHON_RUNNER_BIN_PATH),
                "Build it with scripts/build_python_runner.sh".to_string(),
            ],
            RunnerLaunchError::Flash(err) => err.to_lines(),
        }
    }
}

pub fn launch_python_runner(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    keyboard: &mut CardputerKeyboard<'static>,
    script_path: &Path,
) {
    if let Err(err) = try_launch_python_runner(buffers, script_path) {
        error!("python runner launch failed: {:?}", err);
        show_message_and_wait(buffers, keyboard, "Python Runner", &err.to_lines());
    }
}

fn try_launch_python_runner(
    buffers: &mut DoubleBuffer<SCREEN_WIDTH, SCREEN_HEIGHT>,
    script_path: &Path,
) -> Result<(), RunnerLaunchError> {
    write_run_request(script_path).map_err(RunnerLaunchError::WriteRequest)?;

    let primary_runner = Path::new(PYTHON_RUNNER_BIN_PATH);
    let legacy_runner = Path::new(LEGACY_PYTHON_RUNNER_BIN_PATH);
    let runner_path = if primary_runner.is_file() {
        primary_runner
    } else if legacy_runner.is_file() {
        legacy_runner
    } else {
        return Err(RunnerLaunchError::RunnerMissing);
    };

    chainload::flash_and_reboot(buffers, runner_path).map_err(RunnerLaunchError::Flash)
}
