use lib::{extract_streets, filter, process};
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
        geo_json: bool,
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
            geo_json,
        } => {
            let file = File::open(shared_opts.path)?;
            extract_streets(file, &mut handle, geo_json)?;
        }
    }
    Ok(())
}
