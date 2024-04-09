# LASman in Rust
LAS manipulation tools in Rust

## Tools
Just one tool is available because that's all I need right now!
- clip: Clips points according to polygon(s) defined in a given shapefile.

## Install via Cargo
```bash
cargo install --git https://github.com/konmenel/lasman.git
```

## Usage
```bash
lasman <TOOL> <TOOL_ARGS>
```

For more information:
```bash
lasman -h
```
and for the clip tool
```bash
lasman clip -h
```

## TODO
- Handle polygons with holes
- Handle all PolygonRings in Polygon
