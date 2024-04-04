#!/bin/env python3
import argparse
from math import ceil
from typing import Callable, Iterable
import geopandas as gpd
import laspy
from shapely.geometry.point import Point
from alive_progress import alive_bar


DEFAULT_CHUNK: int = 100_000


def get_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="clip", description="Clips a las file according to polygons"
    )

    parser.add_argument(
        "-i", "--input", required=True, help="The name of th input las file."
    )
    parser.add_argument(
        "-o", "--output", required=True, help="The name of the output las file."
    )
    parser.add_argument(
        "-s",
        "--shapefile",
        required=True,
        help="The name of the shapefile that contains the polygons.",
    )
    parser.add_argument(
        "--chunk-size",
        type=int,
        default=DEFAULT_CHUNK,
        help=f"The size of the reading chunk. Default: {DEFAULT_CHUNK:,} points.",
    )
    parser.add_argument(
        "--external",
        action="store_true",
        help="Clips points outside the polygons in the shapefile.",
    )
    parser.add_argument(
        "--intersection",
        action="store_true",
        help=(
            "Clips points that are in the intersection of all polygons "
            "of the shapefile."
        ),
    )
    return parser


def _is_inside(polygons: gpd.GeoSeries, x: float, y: float, intersection: bool) -> bool:
    reduce_func: Callable[[Iterable], bool] = all if intersection else any
    return reduce_func((p.contains(Point(x, y)) for p in polygons))


def _main_loop_with_progress_bar(
    writer: laspy.LasWriter,
    reader: laspy.LasReader,
    polygons: gpd.GeoSeries,
    chunk_size: int,
    external: bool,
    intersection: bool,
) -> None:
    npoints: int = reader.header.point_count
    monitor_str = "{count}k/{total}k points done [{percent:.1%}]"
    with alive_bar(ceil(npoints / chunk_size), monitor=monitor_str) as bar:
        for points in reader.chunk_iterator(chunk_size):
            contained = [
                _is_inside(polygons, x, y, intersection) ^ external
                for x, y in zip(points.x, points.y)
            ]
            writer.write_points(points[contained])
            bar()


def main(args=None) -> int:
    if not args:
        parser: argparse.ArgumentParser = get_parser()
        args = parser.parse_args()

    data: gpd.GeoDataFrame = gpd.read_file(args.shapefile)
    polygons: gpd.GeoSeries = data.loc[:, "geometry"]

    with laspy.open(args.input) as reader:
        with laspy.open(args.output, mode="w", header=reader.header) as writer:
            _main_loop_with_progress_bar(
                writer,
                reader,
                polygons,
                args.chunk_size,
                args.external,
                args.intersection,
            )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
