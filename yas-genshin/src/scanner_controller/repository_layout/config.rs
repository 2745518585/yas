use clap::arg;

#[derive(Clone, clap::Args)]
pub struct GenshinRepositoryScannerLogicConfig {
    /// Max rows to scan
    #[arg(id = "max-row", long = "max-row", help = "最大扫描行数", default_value_t = -1)]
    pub max_row: i32,

    // todo move to another scanner
    /// Will the scanner capture only?
    // pub capture_only: bool,

    /// The time to wait for scrolling. Consider increasing this value if the scrolling is not correct
    #[arg(id = "scroll-delay", long = "scroll-delay", help = "翻页时滚轮停顿时间（ms）（翻页不正确可以考虑加大该选项）", default_value_t = 80)]
    pub scroll_delay: i32,

    /// Dump the captured image
    // pub dump_mode: bool,

    /// The maximum time to wait for switching to the next item
    #[arg(id = "max-wait-switch-item", long = "max-wait-switch-item", help = "切换物品最大等待时间（ms）", default_value_t = 800)]
    pub max_wait_switch_item: i32,

    /// The time to wait for switching to the next item in cloud game
    #[arg(id = "cloud-wait-switch-item", long = "cloud-wait-switch-item", help = "云游戏切换物品等待时间（ms）", default_value_t = 300)]
    pub cloud_wait_switch_item: i32,
}

impl Default for GenshinRepositoryScannerLogicConfig {
    fn default() -> Self {
        GenshinRepositoryScannerLogicConfig {
            max_row: -1,
            // capture_only: false,
            scroll_delay: 80,
            // number: -1,
            // dump_mode: false,
            max_wait_switch_item: 800,
            cloud_wait_switch_item: 300,
        }
    }
}

impl GenshinRepositoryScannerLogicConfig {
    pub fn validate_complete_scan(&self) -> anyhow::Result<()> {
        if self.max_row >= 0 {
            anyhow::bail!("完整扫描不支持有限的 --max-row，请移除此选项或设为 -1");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_scan_rejects_finite_row_limits() {
        for max_row in [0, 1, 2, i32::MAX] {
            let config = GenshinRepositoryScannerLogicConfig {
                max_row,
                ..Default::default()
            };
            assert!(config.validate_complete_scan().is_err());
        }
        assert!(GenshinRepositoryScannerLogicConfig::default()
            .validate_complete_scan()
            .is_ok());
    }
}
