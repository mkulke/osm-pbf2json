use lib::{filter, process};
use std::error::Error;
use std::fs::File;
use std::io;
use structopt::StructOpt;

mod lib;

#[derive(StructOpt)]
struct Cli {
    #[structopt(short = "t", long = "tags")]
    tags: String,
    #[structopt(parse(from_os_str))]
    path: std::path::PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::from_args();
    let file = File::open(args.path)?;
    let groups = filter::parse(args.tags);
    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout);
    process(file, &mut handle, &groups)?;
    Ok(())
}
