#!/bin/env python3
import argparse
import geopandas as gpd
import laspy
from shapely.geometry.point import Point
from alive_progress import alive_bar


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
        default=100_000,
        help="The size of the reading chunk. Default: 100,000 points.",
    )

    return parser


def _is_inside(polygons: gpd.GeoSeries, x: float, y: float) -> bool:
    return any((p.contains(Point(x, y)) for p in polygons))


def _get_contained_list(
    polygons: gpd.GeoSeries, points: laspy.lasreader.PointChunkIterator
) -> list[bool]:
    return [_is_inside(polygons, x, y) for x, y in zip(points.x, points.y)]


def _write_loop_with_progress_bar(
    writer: laspy.LasWriter,
    reader: laspy.LasReader,
    polygons: gpd.GeoSeries,
    chunk_size: int,
) -> None:
    npoints: int = reader.header.point_count
    monitor_str = "{count}k/{total}k points done [{percent:.1f}%]"
    with alive_bar(npoints // chunk_size, monitor=monitor_str) as bar:
        for points in reader.chunk_iterator(chunk_size):
            contained = _get_contained_list(polygons, points)
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
            _write_loop_with_progress_bar(writer, reader, polygons, args.chunk_size)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
