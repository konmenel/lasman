mod cli_clip;
mod clip;

use anyhow::Result;
use clap::{Parser, Subcommand};
use cli_clip::ClipCliArgs;

use crate::clip::{clip, Strategy};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    tool: Tool,
}

#[derive(Subcommand, Debug)]
enum Tool {
    /// Clips points according to polygon(s) defined in a given
    /// shapefile
    Clip(ClipCliArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    Ok(match &cli.tool {
        Tool::Clip(args) => {
            let lasfile = &args.input;
            let shapefile = &args.shapefile;
            let outfile = &args.output;
            let strategy = if args.intersect {
                Strategy::Intersection
            } else {
                Strategy::Union
            };
            let external = args.external;
            let nthreads = args.threads;
            let chunk_size = args.chuck_size;
            clip(
                lasfile, shapefile, outfile, strategy, external, nthreads, chunk_size,
            )?
        }
    })
}
