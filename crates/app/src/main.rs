//! MathSeea 原生桌面应用及无窗口导出入口.

mod app;
mod cli;
mod document;
mod theme;

use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("MathSeea: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = cli::Cli::parse(std::env::args_os().skip(1)).map_err(|error| error.to_string())?;
    if args.help {
        print!("{}", cli::HELP);
        return Ok(());
    }
    if let Some(path) = args.export {
        let source = args.source.ok_or("导出需要图文件或 Mathlib 源目录")?;
        let document = document::load(source).map_err(|error| error.to_string())?;
        document::export_new(&path, &document).map_err(|error| error.to_string())?;
        for warning in &document.warnings {
            eprintln!("说明: {warning}");
        }
        println!(
            "已导出 {} 个模块, {} 条导入关系: {}",
            document.graph.nodes.len(),
            document.graph.edges.len(),
            path.display()
        );
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([640.0, 420.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let source = Some(args.source.unwrap_or(cli::Source::Bundled));
    eframe::run_native(
        "MathSeea",
        options,
        Box::new(move |context| Ok(Box::new(app::Explorer::new(context, source)))),
    )
    .map_err(|error| format!("无法打开图形窗口: {error}"))
}
