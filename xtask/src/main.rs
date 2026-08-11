use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, ErrorKind, Write};
use std::path::PathBuf;

use dyndo_core::asset::{ASSET_SCHEMA_URL, Asset};
use schemars::schema_for;

fn main() -> Result<(), Box<dyn Error>> {
    let output = parse_asset_schema_command(env::args().skip(1))?;
    write_asset_schema(File::create(output)?)
}

fn parse_asset_schema_command(args: impl Iterator<Item = String>) -> io::Result<PathBuf> {
    let mut args = args;
    if args.next().as_deref() != Some("asset-schema") || args.next().as_deref() != Some("--output")
    {
        return Err(usage_error());
    }
    let Some(output) = args.next() else {
        return Err(usage_error());
    };
    if args.next().is_some() {
        return Err(usage_error());
    }
    Ok(output.into())
}

fn usage_error() -> io::Error {
    io::Error::new(
        ErrorKind::InvalidInput,
        "usage: cargo xtask asset-schema --output <path>",
    )
}

fn write_asset_schema(mut output: impl Write) -> Result<(), Box<dyn Error>> {
    let mut schema = serde_json::to_value(schema_for!(Asset))?;
    let Some(schema_object) = schema.as_object_mut() else {
        return Err("Schemars generated a non-object schema".into());
    };
    schema_object.insert("$id".to_owned(), ASSET_SCHEMA_URL.into());

    serde_json::to_writer_pretty(&mut output, &schema)?;
    writeln!(output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::parse_asset_schema_command;

    #[test]
    fn asset_schema_command_requires_an_output_path() {
        let parsed = parse_asset_schema_command(
            ["asset-schema", "--output", "schema.json"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();

        assert_eq!(parsed, PathBuf::from("schema.json"));
    }

    #[test]
    fn asset_schema_command_rejects_unexpected_arguments() {
        let result = parse_asset_schema_command(["asset-schema"].into_iter().map(str::to_owned));

        assert!(result.is_err());
    }
}
