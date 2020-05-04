use filter::{filter, Group};
use geo::prelude::*;
use geo_types::LineString;
use osmpbfreader::objects::{NodeId, OsmId, OsmObj, Tags};
use osmpbfreader::OsmPbfReader;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{Read, Seek, Write};

pub mod filter;

#[derive(Serialize, Deserialize)]
struct JSONNode {
    id: i64,
    #[serde(rename = "type")]
    osm_type: &'static str,
    lat: f64,
    lon: f64,
    tags: Tags,
}

#[derive(Serialize, Deserialize)]
struct Location {
    lat: f64,
    lon: f64,
}

#[derive(Serialize, Deserialize)]
struct Bounds {
    e: f64,
    n: f64,
    s: f64,
    w: f64,
}

#[derive(Serialize, Deserialize)]
struct JSONWay {
    id: i64,
    #[serde(rename = "type")]
    osm_type: &'static str,
    tags: Tags,
    centroid: Option<Location>,
    bounds: Option<Bounds>,
}

fn get_line_string(objs: &BTreeMap<OsmId, OsmObj>, node_ids: &[NodeId]) -> LineString<f64> {
    let coordinates: Vec<(f64, f64)> = node_ids
        .iter()
        .filter_map(|id| {
            let obj = objs.get(&OsmId::Node(*id))?;
            obj.node()
        })
        .map(|node| (node.lon(), node.lat()))
        .collect();
    coordinates.into()
}

fn get_bounds(objs: &BTreeMap<OsmId, OsmObj>, node_ids: &[NodeId]) -> Option<Bounds> {
    let line_string = get_line_string(objs, node_ids);
    let rect = line_string.bounding_rect()?;
    Some(Bounds {
        e: rect.max().x,
        n: rect.max().y,
        s: rect.min().y,
        w: rect.min().x,
    })
}

fn get_centroid(objs: &BTreeMap<OsmId, OsmObj>, node_ids: &[NodeId]) -> Option<Location> {
    let line_string = get_line_string(objs, node_ids);
    let point = line_string.centroid()?;
    Some(Location {
        lat: point.lat(),
        lon: point.lng(),
    })
}

pub fn process(
    file: impl Seek + Read,
    mut writer: impl Write,
    groups: &[Group],
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, groups))?;
    for obj in objs.values() {
        if !filter(obj, groups) {
            continue;
        }

        match obj {
            OsmObj::Node(node) => {
                let jn = JSONNode {
                    osm_type: "node",
                    id: node.id.0,
                    lat: node.lat(),
                    lon: node.lon(),
                    tags: node.tags.clone(),
                };
                let jn_str = to_string(&jn)?;
                writeln!(writer, "{}", jn_str)?;
            }
            OsmObj::Way(way) => {
                let centroid = get_centroid(&objs, &way.nodes);
                let bounds = get_bounds(&objs, &way.nodes);
                let jw = JSONWay {
                    osm_type: "way",
                    id: way.id.0,
                    tags: way.tags.clone(),
                    centroid,
                    bounds,
                };
                let jw_str = to_string(&jw)?;
                writeln!(writer, "{}", jw_str)?;
            }
            _ => (),
        }
    }
    Ok(())
}
