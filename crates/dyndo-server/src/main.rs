//! Thin binary: load config and print it. Serving is wired in the final task.

fn main() -> anyhow::Result<()> {
    let config = dyndo_server::Config::load()?;
    println!("{config:?}");
    Ok(())
}
