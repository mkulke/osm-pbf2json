use self::geo::{get_compound_coordinates, get_geo_info, Bounds, Location};
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

#[derive(Serialize, Deserialize)]
struct JSONRelation {
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
            .filter_map(|&id| {
                let obj = objs.get(&id.into())?;
                let node = obj.node()?;
                Some((node.lon(), node.lat()))
            })
            .collect()
    }
}

impl OsmExt for Relation {
    fn get_coordinates(&self, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<(f64, f64)> {
        let coordinates = self
            .refs
            .iter()
            .filter_map(|osm_ref| {
                let obj = objs.get(&osm_ref.member)?;
                let coordinates = match obj {
                    OsmObj::Node(node) => vec![(node.lon(), node.lat())],
                    OsmObj::Way(way) => way.get_coordinates(objs),
                    OsmObj::Relation(_) => unimplemented!(),
                };
                Some(coordinates)
            })
            .flatten()
            .collect();
        get_compound_coordinates(coordinates)
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
            OsmObj::Relation(relation) => {
                let coordinates = relation.get_coordinates(&objs);
                let (centroid, bounds) = get_geo_info(coordinates);
                let jr = JSONRelation {
                    osm_type: "relation",
                    id: relation.id.0,
                    tags: relation.tags.clone(),
                    centroid,
                    bounds,
                };
                let jr_str = to_string(&jr)?;
                writeln!(writer, "{}", jr_str)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod get_coordinates {
    use super::*;
    use osmpbfreader::objects::{Node, NodeId, Ref, Relation, RelationId, Tags};
    use std::collections::BTreeMap;

    fn get_node(id: NodeId, lng: i32, lat: i32) -> Node {
        let tags = Tags::new();
        let decimicro_lat = lat * 10_000_000;
        let decimicro_lon = lng * 10_000_000;
        Node {
            id,
            tags,
            decimicro_lat,
            decimicro_lon,
        }
    }

    #[test]
    fn relation_without_refs() {
        let obj_map = BTreeMap::new();
        let id = RelationId(42);
        let tags = Tags::new();
        let refs = vec![];
        let rel = Relation { id, tags, refs };
        let coordinates = rel.get_coordinates(&obj_map);
        assert_eq!(coordinates.len(), 0);
    }

    #[test]
    fn relation_with_one_node() {
        let node_id = NodeId(41);
        let node = get_node(node_id, 5, 49);
        let mut obj_map = BTreeMap::new();
        obj_map.insert(node_id.into(), node.into());
        let id = RelationId(42);
        let tags = Tags::new();
        let node_ref = Ref {
            member: node_id.into(),
            role: "something".to_string(),
        };
        let refs = vec![node_ref];
        let rel = Relation { id, tags, refs };
        let coordinates = rel.get_coordinates(&obj_map);
        assert_eq!(coordinates, vec![(5., 49.)]);
    }

    #[test]
    fn relation_with_multiple_nodes() {
        let coords = vec![(6, 52), (6, 50), (8, 50), (8, 52), (7, 51)];

        // Node 4 is located in the middle of a grid
        // and should hence be ignored.
        //
        // 0     3
        //
        //    4
        //
        // 1     2

        let mut obj_map: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        let mut node_ids: Vec<NodeId> = vec![];
        for (i, coord) in coords.iter().enumerate() {
            let (lng, lat) = coord;
            let node_id = NodeId((i as i64) + 1);
            let node = get_node(node_id, *lng, *lat);
            obj_map.insert(node_id.into(), node.into());
            node_ids.push(node_id);
        }
        let id = RelationId(42);
        let tags = Tags::new();
        let refs = node_ids
            .into_iter()
            .map(|node_id| Ref {
                member: node_id.into(),
                role: "something".to_string(),
            })
            .collect();
        let rel = Relation { id, tags, refs };
        let coordinates = rel.get_coordinates(&obj_map);

        // We expect a simplified closed rectangle.
        //
        // 3-----2
        // |     |
        // |     |
        // |     |
        // 0/4---1

        assert_eq!(
            coordinates,
            vec![(6., 50.), (8., 50.), (8., 52.), (6., 52.), (6., 50.)]
        );
    }
}
