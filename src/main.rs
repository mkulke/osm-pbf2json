use lib::output::Output;
use lib::{boundaries, filter, process, streets};
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
        #[structopt(short, long)]
        boundary: Option<u8>,
    },
    Boundaries {
        #[structopt(flatten)]
        shared_opts: SharedOpts,
        #[structopt(short, long)]
        geojson: bool,
        #[structopt(short, long)]
        levels: Option<Vec<u8>>,
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
            boundary,
        } => {
            let file = File::open(shared_opts.path)?;
            let streets = streets(file, name, boundary)?;
            if geojson {
                streets.write_geojson(&mut handle)?;
            } else {
                streets.write_json_lines(&mut handle)?;
            }
        }
        Cli::Boundaries {
            shared_opts,
            levels,
            geojson,
        } => {
            let file = File::open(shared_opts.path)?;
            let boundaries = boundaries(file, levels)?;
            if geojson {
                boundaries.write_geojson(&mut handle)?;
            } else {
                boundaries.write_json_lines(&mut handle)?;
            }
        }
    }
    Ok(())
}
