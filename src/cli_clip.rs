use clap::Args;

#[derive(Args, Debug)]
pub struct ClipCliArgs {
    /// The input las file
    #[arg(short, long)]
    pub input: String,

    /// The output las file
    #[arg(short, long)]
    pub output: String,

    #[arg(short, long)]
    /// The shapefile that contains the polygons
    pub shapefile: String,

    /// If set only the points that are outside the polygons
    /// will be inluded
    #[arg(long)]
    pub external: bool,

    /// Only points inside the intersection (if there is one) of
    /// the polygons will be included. By default, points in any of
    /// the polygons are included.
    #[arg(long)]
    pub intersect: bool,
    
    /// The size of the chuck (number of points) that will be read
    /// per iteration while processing
    #[arg(long, default_value_t = 1_234_567)]
    pub chunk_size: u64,

    /// The number of threads. If 0, all avaialble cores will be used
    #[arg(long, default_value_t = 0)]
    pub threads: usize,
}
