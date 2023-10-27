use std::f64;
use std::io::BufReader;

use anyhow::Result;
use fs_err::File;
use geojson::{FeatureReader, Value};
use mvt::{GeomEncoder, GeomType, MapGrid, Tile, TileId};
use pmtiles2::{util::tile_id, Compression, PMTiles, TileType};
use pointy::Transform;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        panic!("Pass in a .geojson file");
    }

    let web_mercator_transform = MapGrid::default();

    // Everything
    let current_tile_id = TileId::new(0, 0, 0)?;
    let transform = web_mercator_transform.tile_transform(current_tile_id);
    let mut tile = Tile::new(4096);

    let mut layer = tile.create_layer("layer1");

    let reader = FeatureReader::from_reader(BufReader::new(File::open(&args[1])?));

    for feature in reader.features() {
        let feature = feature?;
        let mut b = GeomEncoder::new(GeomType::Linestring, Transform::default());

        if let Some(geometry) = feature.geometry {
            if let Value::LineString(line_string) = geometry.value {
                for pt in line_string {
                    //Transform to mercator
                    let mercator_pt = forward([pt[0], pt[1]]);
                    // Transform to 0-1 tile coords (not sure why this doesnt work with passing the
                    // transform through)
                    let transformed_pt = transform * (mercator_pt[0], mercator_pt[1]);
                    b = b.point(transformed_pt.x * 4096.0, transformed_pt.y * 4096.0)?;
                }
            }
        }
        let id = layer.num_features() as u64;
        let mut write_feature = layer.into_feature(b.encode()?);
        write_feature.set_id(id);
        // TODO actual things
        write_feature.add_tag_string("key", "value");

        // Very weird pattern, but OK
        layer = write_feature.into_layer();
    }

    println!("Got {} features", layer.num_features());
    tile.add_layer(layer)?;

    let mut pmtiles = PMTiles::new(TileType::Mvt, Compression::None);
    let metadata = serde_json::json!(
        {
            "antimeridian_adjusted_bounds":"-180,-90,180,90",
            "vector_layers": [
            {
                "id": "layer1",
                "minzoom": 0,
                "maxzoom": 1,
                "fields": {
                    "key": "String"
                }
            }
            ]
        }
    );

    pmtiles.meta_data = Some(metadata);

    pmtiles.add_tile(
        tile_id(
            current_tile_id.z() as u8,
            current_tile_id.x() as u64,
            current_tile_id.y() as u64,
        ),
        tile.to_bytes()?,
    );

    pmtiles.min_latitude = -90.0;
    pmtiles.min_longitude = -180.0;
    pmtiles.max_latitude = 90.0;
    pmtiles.max_longitude = 180.0;
    pmtiles.center_zoom = 0;
    let mut file = File::create("out.pmtiles")?;
    pmtiles.to_writer(&mut file)?;

    Ok(())
}

static A: f64 = 6378137.0;
static MAXEXTENT: f64 = 20037508.342789244;
static D2R: f64 = f64::consts::PI / 180.0;

// WGS84 to Mercator
fn forward(c: [f64; 2]) -> [f64; 2] {
    [
        (A * c[0] * D2R).max(-MAXEXTENT).min(MAXEXTENT) as f64,
        (A * (((f64::consts::PI * 0.25f64) + (0.5f64 * c[1] * D2R)).tan()).ln())
            .max(-MAXEXTENT)
            .min(MAXEXTENT) as f64,
    ]
}