use lib::{extract_roads, filter, process};
use std::error::Error;
use std::fs::File;
use std::io;
use structopt::StructOpt;

mod lib;

#[derive(StructOpt)]
struct SharedOpts {
    #[structopt(parse(from_os_str))]
    path: std::path::PathBuf,
}

#[derive(StructOpt)]
enum Cli {
    Objects {
        #[structopt(short, long)]
        tags: String,
        #[structopt(flatten)]
        shared_opts: SharedOpts,
    },
    Streets {
        #[structopt(flatten)]
        shared_opts: SharedOpts,
        #[structopt(short, long)]
        geojson: bool,
        #[structopt(short, long)]
        name: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout);
    let args = Cli::from_args();
    match args {
        Cli::Objects { tags, shared_opts } => {
            let file = File::open(shared_opts.path)?;
            let groups = filter::parse(tags);
            process(file, &mut handle, &groups)?;
        }
        Cli::Streets {
            shared_opts,
            geojson,
            name,
        } => {
            let file = File::open(shared_opts.path)?;
            extract_roads(file, &mut handle, geojson, name)?;
        }
    }
    Ok(())
}
