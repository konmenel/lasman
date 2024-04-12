pub extern crate las;
pub extern crate shapefile;
use anyhow::{Context, Ok, Result};
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};
use las::{Header, Point as lasPoint, Read, Reader, Write, Writer};
use num_format::{Locale, ToFormattedString};
use rayon::prelude::*;
use shapefile::record::polygon::GenericPolygon;
use shapefile::record::traits::{GrowablePoint, HasXY, ShrinkablePoint};
use shapefile::{Point, Polygon, PolygonRing, Shape};
use std::fmt;
use std::io::{self, Write as StdWrte};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Copy, Clone)]
pub enum Strategy {
    Union,
    Intersection,
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let strategy_str = match self {
            Strategy::Intersection => "Intersection",
            Strategy::Union => "Union",
        };
        write!(f, "{strategy_str}")
    }
}

pub fn polymz2poly<PointType>(polym: &GenericPolygon<PointType>) -> Polygon
where
    PointType: ShrinkablePoint + GrowablePoint + PartialEq + HasXY + Copy,
{
    let poly_rings = polym
        .rings()
        .iter()
        .map(|ring| {
            PolygonRing::Outer(
                ring.points()
                    .iter()
                    .map(|&p| Point::new(p.x(), p.y()))
                    .collect::<Vec<Point>>(),
            )
        })
        .collect::<Vec<PolygonRing<Point>>>();
    Polygon::with_rings(poly_rings)
}

fn transform_point(point: &mut Point, offsets: &[f64; 2]) {
    point.x -= offsets[0];
    point.y -= offsets[1];
}

pub fn winding_number(point: &Point, polygon: &Polygon, offsets: &[f64; 2]) -> i32 {
    let mut wn = 0;
    let mut point: Point = Point::new(point.x, point.y);
    transform_point(&mut point, offsets);
    for window in polygon.rings()[0].points().windows(2) {
        let mut p1: Point = window.first().unwrap().clone();
        transform_point(&mut p1, offsets);
        let mut p2: Point = window.last().unwrap().clone();
        transform_point(&mut p2, offsets);

        if point.x > p1.x && point.x > p2.x {
            continue;
        }
        if point.y > p1.y.max(p2.y) {
            continue;
        }
        if point.y < p1.y.min(p2.y) {
            continue;
        }
        if p1.y == p2.y {
            continue;
        }

        // Check for intesection
        let x_inters = (point.y - p1.y) * (p2.x - p1.x) / (p2.y - p1.y) + p1.x;
        if p1.x == p2.x || x_inters >= point.x {
            if p2.y > p1.y {
                wn += 1;
            } else {
                wn -= 1;
            }
        }
    }
    wn
}

pub fn is_point_in_poly(point: &Point, polygon: &Polygon, offsets: &[f64; 2]) -> bool {
    winding_number(point, polygon, offsets) != 0
}

fn load_polygons<P: AsRef<Path>>(shapefile: P) -> Result<Vec<Polygon>> {
    let mut reader = shapefile::ShapeReader::from_path(shapefile.as_ref()).with_context(|| {
        format!(
            "Cannot open shapefile \"{}\"",
            shapefile.as_ref().to_string_lossy()
        )
    })?;
    Ok(reader
        .iter_shapes()
        .map_while(|shape| shape.ok())
        .filter_map(|s| match s {
            Shape::Polygon(poly) => Some(poly.clone()),
            Shape::PolygonM(poly) => Some(polymz2poly(&poly)),
            Shape::PolygonZ(poly) => Some(polymz2poly(&poly)),
            _ => None,
        })
        .collect())
}

fn filter_fn(
    strategy: Strategy,
    polygons: &Vec<Polygon>,
    point: &lasPoint,
    external: bool,
    offsets: &[f64; 2],
) -> bool {
    let point = Point::new(point.x, point.y);
    let op = |poly| is_point_in_poly(&point, poly, offsets) != external;
    match strategy {
        Strategy::Union => polygons.iter().any(op),
        Strategy::Intersection => polygons.iter().all(op),
    }
}

fn print_info<P: AsRef<Path> + std::fmt::Display>(
    lasfile: P,
    shapefile: P,
    outfile: P,
    strategy: Strategy,
    external: bool,
    nthreads: usize,
    read_chunk: u64,
) {
    let nthreads: usize = if nthreads > 0 {
        nthreads
    } else {
        rayon::current_num_threads()
    };
    println!(
        "[INFO] LAS file path: \"{}\"",
        lasfile.as_ref().to_string_lossy()
    );
    println!(
        "[INFO] shapefile path: \"{}\"",
        shapefile.as_ref().to_string_lossy()
    );
    println!(
        "[INFO] Output file path: \"{}\"",
        outfile.as_ref().to_string_lossy()
    );
    println!("[INFO] Clip strategy: {}", strategy);
    println!(
        "[INFO] Cliping {} points",
        if external { "external" } else { "internal" }
    );
    println!("[INFO] Number of threads: {}", nthreads);
    println!(
        "[INFO] Reading chuck: {} points",
        read_chunk.to_formatted_string(&Locale::en)
    );
    println!();
}

fn create_progress_bar(total: u64) -> Result<ProgressBar> {
    let pb_style = ProgressStyle::with_template(
        "{bar:50.yellow} {spinner:.green} {human_pos}k/{human_len}k [{percent}%] points done in {elapsed} (ETA:~{eta})",
    ).with_context(|| format!("Progress bar template failed to load"))?;
    let pb = ProgressBar::new(total);
    pb.set_style(pb_style);
    pb.enable_steady_tick(Duration::from_millis(100));
    Ok(pb)
}

pub fn clip<P: AsRef<Path> + std::fmt::Display>(
    lasfile: P,
    shapefile: P,
    outfile: P,
    strategy: Strategy,
    external: bool,
    nthreads: usize,
    read_chunk: u64,
) -> Result<()> {
    if nthreads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(nthreads)
            .build_global()
            .with_context(|| format!("Cannot set number of nthreads to {nthreads}"))?;
    }
    print_info(
        &lasfile, &shapefile, &outfile, strategy, external, nthreads, read_chunk,
    );

    // Check if outfile already exists
    if Path::new(outfile.as_ref()).try_exists()? {
        println!("Output file \"{}\" already exists.", outfile);
        let mut user_inpur = String::new();

        while !["y", "yes", "n", "no"].contains(&user_inpur.trim().to_lowercase().as_str()) {
            user_inpur.clear();
            print!("Overwrite [y/n]: ");
            io::stdout().flush()?;
            io::stdin().read_line(&mut user_inpur).unwrap();
        }

        if ["n", "no"].contains(&user_inpur.trim().to_lowercase().as_str()) {
            println!("Run cancelled!");
            return Ok(());
        }
    }

    // Getting polygons
    let polygons = load_polygons(&shapefile)?;
    println!(
        "[1/2] {} polygon{} loaded from \"{}\".",
        polygons.len(),
        if polygons.len() > 1 { "s" } else { "" },
        shapefile.as_ref().to_string_lossy()
    );

    // Open input and output las files
    let mut reader = Reader::from_path(lasfile.as_ref())
        .with_context(|| format!("Cannot open las file \"{lasfile}\""))?;
    let out_header: Header = reader.header().clone();
    let mut writer = Writer::from_path(outfile.as_ref(), out_header)
        .with_context(|| format!("Cannot open las output file \"{outfile}\""))?;

    // Prepare progress bar
    let total: u64 = (reader.header().number_of_points() as f64 / 1000.0).ceil() as u64;
    let pb: ProgressBar = create_progress_bar(total)?;

    // Main loop
    let started = Instant::now();

    println!("[2/2] Clipping points");
    let points_total: u64 = reader.header().number_of_points();
    // let [scales, offsets] = get_scale_and_offsets(reader.header())?;
    let offsets: [f64; 2] = [
        reader.header().bounds().min.x,
        reader.header().bounds().min.x,
    ];
    let mut points_processes = 0;
    while points_processes < points_total {
        let points = reader.read_n(read_chunk.min(points_total - points_processes))?;
        let contained: Vec<&las::Point> = points
            .par_iter()
            .filter(|&pnt| filter_fn(strategy, &polygons, pnt, external, &offsets))
            .collect();

        for &p in contained.iter() {
            writer
                .write(p.clone())
                .with_context(|| format!("Error while writing points"))?;
        }
        points_processes += points.len() as u64;
        pb.set_position(points_processes / 1000);
    }
    pb.finish();

    println!("Done in {}", HumanDuration(started.elapsed()),);
    println!(
        "Number of points written to \"{}\": {}",
        outfile,
        writer
            .header()
            .number_of_points()
            .to_formatted_string(&Locale::en)
    );
    Ok(())
}
