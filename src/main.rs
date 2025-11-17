use color_eyre::eyre;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    warpx_cli::run()?;
    Ok(())
}
