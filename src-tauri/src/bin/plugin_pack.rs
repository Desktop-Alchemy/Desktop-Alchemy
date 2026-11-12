use anyhow::{Context, Result};
use std::path::PathBuf;
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1).filter(|a| a != "--");
    let source = PathBuf::from(
        args.next()
            .context("用法：pnpm plugin:pack -- <插件目录> [输出.m18plugin]")?,
    )
    .canonicalize()?;
    let destination = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| source.with_extension("m18plugin"));
    let destination = if destination.is_absolute() {
        destination
    } else {
        std::env::current_dir()?.join(destination)
    };
    desktop_alchemy_core::plugin_package::pack(&source, &destination)?;
    println!("{}", destination.display());
    Ok(())
}
