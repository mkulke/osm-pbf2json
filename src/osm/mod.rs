use self::geo::{get_geo_info, Bounds, Location};
use filter::{filter, Group};
use osmpbfreader::objects::{OsmId, OsmObj, Relation, Tags, Way};
use osmpbfreader::OsmPbfReader;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{Read, Seek, Write};

pub mod filter;
mod geo;

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
struct JSONWay {
    id: i64,
    #[serde(rename = "type")]
    osm_type: &'static str,
    tags: Tags,
    centroid: Option<Location>,
    bounds: Option<Bounds>,
}

impl OsmExt for Way {
    fn get_coordinates(&self, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<(f64, f64)> {
        self.nodes
            .iter()
            .filter_map(|id| {
                let obj = objs.get(&OsmId::Node(*id))?;
                obj.node()
            })
            .map(|node| (node.lon(), node.lat()))
            .collect()
    }
}

impl OsmExt for Relation {
    fn get_coordinates(&self, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<(f64, f64)> {
        let mut ref_objs = vec![];
        for osm_ref in &self.refs {
            if let Some(obj) = objs.get(&osm_ref.member) {
                ref_objs.push(obj);
            }
        }
        unimplemented!();
    }
}

trait OsmExt {
    fn get_coordinates(&self, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<(f64, f64)>;
}

pub fn process(
    file: impl Seek + Read,
    mut writer: impl Write,
    groups: &[Group],
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, groups))?;

    for obj in objs.values() {
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
                    tags: node.tags.clone(),
                };
                let jn_str = to_string(&jn)?;
                writeln!(writer, "{}", jn_str)?;
            }
            OsmObj::Way(way) => {
                let coordinates = way.get_coordinates(&objs);
                let (centroid, bounds) = get_geo_info(coordinates);
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
            OsmObj::Relation(_relation) => (),
        }
    }
    Ok(())
}
