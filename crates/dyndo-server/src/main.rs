//! dyndo-server binary: load config and print it. Serving is wired in the final task.

mod config;
mod error;
mod path;
mod routes;
mod segment;
mod state;

#[cfg(test)]
mod tests;

use config::Config;

fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    println!("{config:?}");
    Ok(())
}
