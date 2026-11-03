use std::process::ExitCode;
use yas_genshin::application::ArtifactScannerApplication;

fn main() -> ExitCode {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();
    let command =
        yas_application::web::augment_command(ArtifactScannerApplication::build_command());
    yas_application::run_artifacts(command.get_matches(), &[])
}
