use clap::command;
use std::process::ExitCode;
use yas_genshin::application::ArtifactScannerApplication;

fn main() -> ExitCode {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();
    let genshin =
        yas_application::web::augment_command(ArtifactScannerApplication::build_command())
            .name("genshin");
    let matches = command!()
        .subcommand_required(true)
        .subcommand(genshin)
        .get_matches();
    let (_, matches) = matches.subcommand().expect("required subcommand");
    yas_application::run_artifacts(matches.clone(), &["genshin"])
}
