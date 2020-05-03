use super::filter::{filter, Group};
use geo::prelude::*;
use geo_types::LineString;
use osmpbfreader::objects::{NodeId, OsmId, OsmObj, Tags};
use osmpbfreader::OsmPbfReader;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{self, Write};
use std::io::{Read, Seek};

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

struct Meta {
    centroid: Option<Location>,
    bounds: Option<Bounds>,
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

fn build_meta_map(objs: &BTreeMap<OsmId, OsmObj>) -> BTreeMap<OsmId, Meta> {
    let lookup_map: BTreeMap<OsmId, Meta> = objs
        .iter()
        .filter_map(|(id, obj)| {
            let way = obj.way()?;
            let nodes = &way.nodes;
            let centroid = get_centroid(objs, nodes);
            let bounds = get_bounds(objs, nodes);
            let meta = Meta { centroid, bounds };
            Some((*id, meta))
        })
        .collect();
    lookup_map
}

pub fn process_without_clone(
    file: impl Read + Seek,
    groups: &[Group],
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, groups))?;
    let mut meta_map = build_meta_map(&objs);

    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout);

    for (id, obj) in objs {
        if !filter(&obj, groups) {
            continue;
        }

        match obj {
            OsmObj::Node(node) => {
                let jn = JSONNode {
                    osm_type: "node",
                    id: node.id.0,
                    lat: node.lat(),
                    lon: node.lon(),
                    tags: node.tags,
                };
                let jn_str = to_string(&jn)?;
                writeln!(handle, "{}", jn_str)?;
            }
            OsmObj::Way(way) => {
                let Meta { centroid, bounds } = meta_map.remove(&id).unwrap_or(Meta {
                    centroid: None,
                    bounds: None,
                });

                let jw = JSONWay {
                    osm_type: "way",
                    id: way.id.0,
                    tags: way.tags,
                    centroid,
                    bounds,
                };
                let jw_str = to_string(&jw)?;
                writeln!(handle, "{}", jw_str)?;
            }
            _ => (),
        }
    }
    Ok(())
}

// pub fn process(file: impl Seek + Read, groups: &[Group]) -> Result<(), Box<dyn Error>> {
//     let mut pbf = OsmPbfReader::new(file);
//     let objs = pbf.get_objs_and_deps(|obj| filter(obj, groups))?;
//     for obj in objs.values() {
//         if !filter(obj, groups) {
//             continue;
//         }

//         match obj {
//             OsmObj::Node(node) => {
//                 let jn = JSONNode {
//                     osm_type: "node",
//                     id: node.id.0,
//                     lat: node.lat(),
//                     lon: node.lon(),
//                     tags: node.tags.clone(),
//                 };
//                 println!("{}", to_string(&jn).unwrap());
//             }
//             OsmObj::Way(way) => {
//                 let centroid = get_centroid(&objs, &way.nodes);
//                 let bounds = get_bounds(&objs, &way.nodes);
//                 let jw = JSONWay {
//                     osm_type: "way",
//                     id: way.id.0,
//                     tags: way.tags.clone(),
//                     centroid,
//                     bounds,
//                 };
//                 println!("{}", to_string(&jw).unwrap());
//             }
//             _ => (),
//         }
//     }
//     Ok(())
// }
