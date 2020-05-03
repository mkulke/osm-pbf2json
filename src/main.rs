use std::error::Error;
use std::fs::File;
use structopt::StructOpt;

mod filter;
mod osm;

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
    osm::process_without_clone(file, &groups)?;
    // osm::process(file, &groups)?;
    Ok(())
}
