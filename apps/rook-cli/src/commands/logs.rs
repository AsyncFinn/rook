//! `rook logs`: where the log files are.

use rook_engine::data::DataDir;

pub fn run(open: bool) -> anyhow::Result<()> {
    let data = DataDir::locate().ok_or_else(|| anyhow::anyhow!("no data folder"))?;
    if open {
        std::fs::create_dir_all(&data.logs)?;
        let opener = if cfg!(windows) {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        // Explorer exits with 1 even when it opened the folder.
        std::process::Command::new(opener).arg(&data.logs).spawn()?;
        return Ok(());
    }
    match rook_engine::logging::newest_log(&data.logs) {
        Some(path) => println!("{}", path.display()),
        None => anyhow::bail!("no logs yet in {}", data.logs.display()),
    }
    Ok(())
}
