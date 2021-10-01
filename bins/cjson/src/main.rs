use std::{fmt, fs, io, path::Path, process::exit, str::from_utf8};

use structopt::StructOpt;

use link_canonical::{json::Value, Canonical};

#[derive(Debug, StructOpt)]
pub struct Args {
    input: String,
}

fn main() {
    let Args { input } = Args::from_args();
    let input = resolve(input).expect("failed to resolve input");

    match input.parse::<Value>() {
        Ok(val) => match from_utf8(&val.canonical_form().unwrap()) {
            Ok(val) => println!("{}", val),
            Err(err) => bail(err),
        },
        Err(err) => bail(err),
    }
}

fn bail<E>(err: E)
where
    E: fmt::Display,
{
    eprintln!("{}", err);
    exit(1)
}

/// Check if the input provided is a path, otherwise assume it's raw JSON input
fn resolve(input: String) -> Result<String, io::Error> {
    let i = Path::new(&input);

    if i.exists() {
        fs::read_to_string(i)
    } else {
        Ok(input)
    }
}
