use std::env;
use std::error::Error;
use std::fs::File;

mod filter;
mod osm;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];
    let file = File::open(path)?;
    // osm::process(file)?;
    osm::process_without_clone(file)?;
    Ok(())
}
