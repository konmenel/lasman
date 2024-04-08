use anyhow::{Context, Result};
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};
use las::{Read, Reader, Write, Writer};
use num_format::{Locale, ToFormattedString};
use rayon::prelude::*;
use shapefile::record::polygon::GenericPolygon;
use shapefile::record::traits::{GrowablePoint, HasXY, ShrinkablePoint};
use shapefile::{Point, Polygon, PolygonRing, Shape};
use std::fmt;
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

fn polymz2poly<PointType>(polym: &GenericPolygon<PointType>) -> Polygon
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

fn winding_number(point: &las::Point, polygon: &Polygon) -> i32 {
    let mut wn = 0;
    for ring in polygon.rings() {
        for window in ring.points().windows(2) {
            let p1 = window.first().unwrap();
            let p2 = window.last().unwrap();

            // Array to sort from low to high point
            let mut ps: [&Point; 2] = [p1, p2];
            ps.sort_by(|&a, &b| a.y.partial_cmp(&b.y).unwrap());

            // Get Intersection
            let x_intersect = if p1.y == p2.y {
                f64::INFINITY // Dont care about parallel lines.
            } else if p1.x == p2.x {
                p1.x // The simplest case
            } else {
                let k = (p1.y - p2.y) / (p2.x - p1.x);
                let b = p1.y - k * p1.x;
                (point.y - b) / k
            };
            // Check for the x of the intersect
            if x_intersect > point.x && ps[0].y <= point.y && ps[1].y >= point.y {
                let dir = (p2.y > p1.y) as i32 * 2 - 1;
                wn += dir;
            }
        }
    }
    wn
}

fn is_pnt_in_poly(point: &las::Point, polygon: &Polygon) -> bool {
    winding_number(point, polygon) != 0
}

fn load_polygons<P: AsRef<Path>>(shpfile: P) -> Result<Vec<Polygon>> {
    let mut reader = shapefile::ShapeReader::from_path(shpfile.as_ref()).with_context(|| {
        format!(
            "Cannot open shapefile \"{}\"",
            shpfile.as_ref().to_string_lossy()
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
    point: &las::Point,
    external: bool,
) -> bool {
    let mut poly_iter = polygons.iter();
    let op = |poly| is_pnt_in_poly(point, poly) ^ external;
    match strategy {
        Strategy::Union => poly_iter.any(op),
        Strategy::Intersection => poly_iter.all(op),
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
        "[INFO] Clip point external or internal points: {}",
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

    // Getting polygons
    let polygons = load_polygons(&shapefile)?;
    println!(
        "[1/2] {} polygons loaded from \"{}\".",
        polygons.len(),
        shapefile.as_ref().to_string_lossy()
    );

    // Open reader and writer
    let mut reader = Reader::from_path(lasfile.as_ref())
        .with_context(|| format!("Cannot open las file \"{lasfile}\""))?;
    let mut writer = Writer::from_path(outfile.as_ref(), reader.header().clone())
        .with_context(|| format!("Cannot open las output file \"{outfile}\""))?;

    // Prepare progress bar
    let total = (reader.header().number_of_points() as f64 / 1000.0).ceil() as u64;
    let pb = create_progress_bar(total)?;

    // Main loop
    let started = Instant::now();
    println!("[2/2] Clipping points");
    while let Ok(points) = reader.read_n(read_chunk) {
        let contained: Vec<&las::Point> = points
            .par_iter()
            .filter(|&pnt| filter_fn(strategy, &polygons, pnt, external))
            .collect();

        for &p in contained.iter() {
            writer
                .write(p.clone())
                .with_context(|| format!("Error while writing points"))?;
        }
        pb.inc(points.len() as u64 / 1000);
    }
    pb.finish();
    println!("Done in {}", HumanDuration(started.elapsed()));
    Ok(())
}
