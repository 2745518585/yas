use anyhow::Result;
use anyhow::anyhow;
use clap::{command, Arg, ArgAction, ArgMatches, Args};
use log::info;

use yas::export::{AssetEmitter, ExportAssets};
use yas::game_info::{GameInfo, GameInfoBuilder};
use yas::window_info::{load_window_info_repo, WindowInfoRepository};

use crate::artifact::GenshinArtifact;
use crate::export::artifact::{ExportArtifactConfig, GenshinArtifactExporter};
use crate::scanner::{GenshinArtifactScanner, GenshinArtifactScannerConfig};
use crate::scanner_controller::repository_layout::GenshinRepositoryScannerLogicConfig;

pub struct ArtifactScannerApplication {
    arg_matches: ArgMatches,
}

impl ArtifactScannerApplication {
    pub fn new(matches: ArgMatches) -> Self {
        ArtifactScannerApplication {
            arg_matches: matches
        }
    }

    pub fn build_command() -> clap::Command {
        let mut cmd = command!();
        cmd = <ExportArtifactConfig as Args>::augment_args_for_update(cmd);
        cmd = <GenshinArtifactScannerConfig as Args>::augment_args_for_update(cmd);
        cmd = <GenshinRepositoryScannerLogicConfig as Args>::augment_args_for_update(cmd);
        cmd.arg(Arg::new("no-pause").long("no-pause").action(ArgAction::SetTrue)
            .help("退出时不等待按键，多个游戏窗口时必须指定 --hwnd"))
            .arg(Arg::new("hwnd").long("hwnd").value_parser(clap::value_parser!(isize))
                .help("指定已打开的原神窗口句柄"))
    }

    fn get_window_info_repository() -> WindowInfoRepository {
        load_window_info_repo!(
            "../../window_info/windows1600x900.json",
            "../../window_info/windows1280x960.json",
            "../../window_info/windows1440x900.json",
            "../../window_info/windows2100x900.json",
            "../../window_info/windows3440x1440.json",
        )
    }

    // fn init() {
    //     env_logger::Builder::new()
    //         .filter_level(log::LevelFilter::Info)
    //         .init();
    // }

    fn get_game_info(&self) -> Result<GameInfo> {
        let mut builder = GameInfoBuilder::new();
        builder.add_local_window_name("原神")
            .add_local_window_name("Genshin Impact")
            .add_cloud_window_name("云·原神");
        #[cfg(windows)]
        return builder.build_with_window(self.arg_matches.get_one::<isize>("hwnd").copied(),
            !self.arg_matches.get_flag("no-pause"));
        #[cfg(not(windows))]
        builder.build()
    }
}

impl ArtifactScannerApplication {
    pub fn run(&self) -> Result<()> {
        let arg_matches = &self.arg_matches;
        let window_info_repository = Self::get_window_info_repository();
        let game_info = self.get_game_info()?;

        info!("window: {:?}", game_info.window);
        info!("ui: {:?}", game_info.ui);
        info!("cloud: {}", game_info.is_cloud);
        info!("resolution family: {:?}", game_info.resolution_family);

        #[cfg(target_os = "windows")]
        {
            // assure admin
            if !yas::utils::is_admin() {
                return Err(anyhow!("请使用管理员运行"));
            }
        }

        let mut scanner = GenshinArtifactScanner::from_arg_matches(
            &window_info_repository,
            arg_matches,
            game_info.clone()
        )?;

        let result = if arg_matches.get_flag("no-pause") {
            scanner.scan_require_complete()?
        } else {
            scanner.scan()?
        };
        let artifacts = result
            .iter()
            .flat_map(GenshinArtifact::try_from)
            .collect::<Vec<_>>();

        if arg_matches.get_flag("no-pause") && artifacts.len() != result.len() {
            return Err(anyhow!("部分圣遗物识别结果无效，未导出自动导入结果"));
        }

        let exporter = GenshinArtifactExporter::new(arg_matches, &artifacts)?;
        let mut export_assets = ExportAssets::new();
        exporter.emit(&mut export_assets);

        let stats = export_assets.save();
        if arg_matches.get_flag("no-pause") && !stats.failed_items.is_empty() {
            return Err(anyhow!("扫描结果保存失败，未完成自动导入"));
        }
        info!("保存结果：");
        let table = format!("{}", stats);
        // print multiline
        for line in table.lines() {
            info!("{}", line);
        }
        info!("Yas 识别结束，共识别到 {} 件圣遗物。", result.len());

        Ok(())
    }
}
