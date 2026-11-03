pub mod web;

use clap::ArgMatches;
use std::process::ExitCode;
use yas::utils::press_any_key_to_continue;
use yas_genshin::application::ArtifactScannerApplication;

pub fn run_artifacts(matches: ArgMatches, child_prefix: &[&str]) -> ExitCode {
    match web::handle(&matches, child_prefix) {
        Ok(true) => return ExitCode::SUCCESS,
        Err(error) => {
            log::error!("{error:#}");
            return ExitCode::FAILURE;
        },
        Ok(false) => {},
    }
    let no_pause = matches.get_flag("no-pause");
    let result = ArtifactScannerApplication::new(matches).run();
    if let Err(error) = &result {
        log::error!("{error:#}");
    }
    if !no_pause {
        press_any_key_to_continue();
    }
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
