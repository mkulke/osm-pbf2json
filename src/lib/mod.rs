use self::admin::AdminBoundary;
use self::geo::{get_compound_coordinates, get_geo_info, Bounds, Location};
use admin::{get_boundaries, AdminOutput};
use filter::{filter, Condition, Group};
use osmpbfreader::objects::{Node, OsmId, OsmObj, Relation, Tags, Way};
use osmpbfreader::OsmPbfReader;
use rstar::RTree;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{Read, Seek, Write};
use streets::{get_streets, StreetOutput};

mod admin;
pub mod filter;
mod geo;
mod geojson;
mod streets;

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

type SerdeResult = Result<String, serde_json::error::Error>;

trait SerializeParent {
    fn get_coordinates(&self, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<(f64, f64)>;
    fn to_json_string(&self, objs: &BTreeMap<OsmId, OsmObj>) -> SerdeResult;
}

impl SerializeParent for Way {
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

    fn to_json_string(&self, objs: &BTreeMap<OsmId, OsmObj>) -> SerdeResult {
        let coordinates = self.get_coordinates(&objs);
        let (centroid, bounds) = get_geo_info(coordinates);
        let jw = JSONWay {
            osm_type: "way",
            id: self.id.0,
            tags: self.tags.to_owned(),
            centroid,
            bounds,
        };
        to_string(&jw)
    }
}

impl SerializeParent for Relation {
    fn get_coordinates(&self, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<(f64, f64)> {
        let coordinates = self
            .refs
            .iter()
            .filter_map(|osm_ref| {
                let obj = objs.get(&osm_ref.member)?;
                let coordinates = match obj {
                    OsmObj::Node(node) => vec![(node.lon(), node.lat())],
                    OsmObj::Way(way) => way.get_coordinates(objs),
                    OsmObj::Relation(rel) => rel.get_coordinates(objs),
                };
                Some(coordinates)
            })
            .flatten()
            .collect();
        get_compound_coordinates(coordinates)
    }

    fn to_json_string(&self, objs: &BTreeMap<OsmId, OsmObj>) -> SerdeResult {
        let coordinates = self.get_coordinates(&objs);
        let (centroid, bounds) = get_geo_info(coordinates);
        let jr = JSONRelation {
            osm_type: "relation",
            id: self.id.0,
            tags: self.tags.to_owned(),
            centroid,
            bounds,
        };
        to_string(&jr)
    }
}

trait SerializeNode {
    fn to_json_string(&self) -> SerdeResult;
}

impl SerializeNode for Node {
    fn to_json_string(&self) -> SerdeResult {
        let jn = JSONNode {
            osm_type: "node",
            id: self.id.0,
            lat: self.lat(),
            lon: self.lon(),
            tags: self.tags.to_owned(),
        };
        to_string(&jn)
    }
}

fn build_admin_group(levels: Vec<u8>) -> Vec<Group> {
    use Condition::*;

    levels
        .iter()
        .map(|level| {
            let level_match = ValueMatch("admin_level".to_string(), level.to_string());
            let boundary_match = ValueMatch("boundary".to_string(), "administrative".to_string());
            let conditions = vec![boundary_match, level_match];
            Group { conditions }
        })
        .collect()
}

fn build_street_group(name: Option<String>) -> Vec<Group> {
    use Condition::*;

    let values = vec![
        "primary",
        "secondary",
        "tertiary",
        "residential",
        "service",
        "living_street",
        "pedestrian",
    ];

    let name_condition = match name {
        Some(name) => ValueMatch("name".to_string(), name),
        None => TagPresence("name".to_string()),
    };
    values
        .into_iter()
        .map(|val| {
            let highway_match = ValueMatch("highway".to_string(), val.to_string());
            let conditions = vec![highway_match, name_condition.clone()];
            Group { conditions }
        })
        .collect()
}

pub fn extract_hierarchies(
    file: impl Seek + Read,
    writer: &mut dyn Write,
    geo_json: bool,
    levels: Option<Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let default_levels = vec![4, 6, 8, 9, 10];
    let levels = levels.unwrap_or(default_levels);
    let groups = build_admin_group(levels);
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, &groups))?;
    let boundaries = get_boundaries(&objs);
    if geo_json {
        let geojson = boundaries.to_geojson()?;
        writeln!(writer, "{}", geojson)?;
    } else {
        boundaries.write_json_lines(writer)?;
    }
    Ok(())
}

pub fn extract_streets(
    file: impl Seek + Read,
    writer: &mut dyn Write,
    geo_json: bool,
    name: Option<String>,
    boundary: Option<u8>,
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let groups = build_street_group(name);
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, &groups))?;
    let streets = get_streets(&objs);
    let streets = {
        match boundary {
            None => streets,
            Some(level) => {
                let groups = build_admin_group(vec![level]);
                let objs = pbf.get_objs_and_deps(|obj| filter(obj, &groups))?;
                let boundaries = get_boundaries(&objs);
                let tree = RTree::<AdminBoundary>::bulk_load(boundaries);
                streets
                    .into_iter()
                    .flat_map(|mut street| {
                        let matches = street.boundary_matches(&tree);
                        match matches.len() {
                            0 => vec![street],
                            1 => {
                                let boundary = matches[0];
                                street.set_boundary(boundary.name());
                                return vec![street];
                            }
                            _ => matches
                                .iter()
                                .map(|boundary| {
                                    let mut new_street = street.clone();
                                    new_street.set_boundary(boundary.name());
                                    new_street
                                })
                                .collect(),
                        }
                    })
                    .collect()
            }
        }
    };
    if geo_json {
        let geojson = streets.to_geojson()?;
        writeln!(writer, "{}", geojson)?;
    } else {
        streets.write_json_lines(writer)?;
    }
    Ok(())
}

pub fn process(
    file: impl Seek + Read,
    writer: &mut dyn Write,
    groups: &[Group],
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, groups))?;

    for obj in objs.values() {
        if !filter(&obj, groups) {
            continue;
        }

        let json_str = match obj {
            OsmObj::Node(node) => node.to_json_string(),
            OsmObj::Way(way) => way.to_json_string(&objs),
            OsmObj::Relation(relation) => relation.to_json_string(&objs),
        }?;
        writeln!(writer, "{}", json_str)?;
    }
    Ok(())
}

#[cfg(test)]
mod get_coordinates {
    use super::*;
    use osmpbfreader::objects::{Node, NodeId, Ref, Relation, RelationId, Tags, Way, WayId};
    use std::collections::BTreeMap;

    fn create_node(id: NodeId, lng: i32, lat: i32) -> Node {
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

    fn create_relation(id: RelationId, refs: Vec<Ref>) -> Relation {
        let tags = Tags::new();
        Relation { id, tags, refs }
    }

    fn create_way(id: WayId, nodes: Vec<NodeId>) -> Way {
        let tags = Tags::new();
        Way { id, tags, nodes }
    }

    fn add_nodes(
        coordinates: Vec<(i32, i32)>,
        obj_map: &mut BTreeMap<OsmId, OsmObj>,
        node_ids: &mut Vec<NodeId>,
    ) {
        for (i, (lng, lat)) in coordinates.iter().enumerate() {
            let node_id = NodeId((i as i64) + 1);
            let node = create_node(node_id, *lng, *lat);
            obj_map.insert(node_id.into(), node.into());
            node_ids.push(node_id);
        }
    }

    fn create_refs(ids: Vec<OsmId>) -> Vec<Ref> {
        ids.into_iter()
            .map(|id| Ref {
                member: id,
                role: "something".to_string(),
            })
            .collect()
    }

    #[test]
    fn relation_without_refs() {
        let obj_map = BTreeMap::new();
        let id = RelationId(42);
        let rel = create_relation(id, vec![]);
        let coordinates = rel.get_coordinates(&obj_map);
        assert_eq!(coordinates.len(), 0);
    }

    #[test]
    fn relation_with_one_way() {
        let coordinates = vec![(9, 50), (9, 51), (10, 51)];

        // 1     2
        //
        //
        // 0

        let mut obj_map = BTreeMap::new();
        let mut node_ids = vec![];
        add_nodes(coordinates, &mut obj_map, &mut node_ids);

        let way_id = WayId(42);
        let way = create_way(way_id, node_ids);
        obj_map.insert(way_id.into(), way.into());

        let refs = create_refs(vec![way_id.into()]);
        let id = RelationId(43);
        let rel = create_relation(id, refs);

        // we expect a closed triangle

        let coordinates = rel.get_coordinates(&obj_map);
        assert_eq!(
            coordinates,
            vec![(9., 50.), (9., 51.), (10., 51.), (9., 50.)]
        );
    }

    #[test]
    fn relation_with_one_node() {
        let node_id = NodeId(41);
        let node = create_node(node_id, 5, 49);
        let mut obj_map = BTreeMap::new();
        obj_map.insert(node_id.into(), node.into());
        let id = RelationId(42);
        let refs = create_refs(vec![node_id.into()]);
        let rel = create_relation(id, refs);
        let coordinates = rel.get_coordinates(&obj_map);
        assert_eq!(coordinates, vec![(5., 49.)]);
    }

    #[test]
    fn relation_with_multiple_nodes() {
        let coordinates = vec![(6, 52), (6, 50), (8, 50), (8, 52), (7, 51)];

        // Node 4 is located in the middle of a grid
        // and should hence be ignored.
        //
        // 0     3
        //
        //    4
        //
        // 1     2

        let mut obj_map = BTreeMap::new();
        let mut node_ids = vec![];
        add_nodes(coordinates, &mut obj_map, &mut node_ids);

        let id = RelationId(42);
        let refs = create_refs(node_ids.into_iter().map(NodeId::into).collect());
        let rel = create_relation(id, refs);
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

    #[test]
    fn nested_relations() {
        let coordinates = vec![(6, 52), (6, 50)];
        let mut obj_map = BTreeMap::new();
        let mut node_ids = vec![];
        add_nodes(coordinates, &mut obj_map, &mut node_ids);

        let child_id = RelationId(42);
        let refs = create_refs(node_ids.into_iter().map(NodeId::into).collect());
        let child_rel = create_relation(child_id, refs);
        obj_map.insert(child_id.into(), child_rel.into());

        let node_id = NodeId(43);
        let node = create_node(node_id, 8, 52);
        obj_map.insert(node_id.into(), node.into());

        let parent_id = RelationId(44);
        let refs = create_refs(vec![child_id.into(), node_id.into()]);
        let parent_rel = create_relation(parent_id, refs);

        let coordinates = parent_rel.get_coordinates(&obj_map);

        assert_eq!(
            coordinates,
            vec![(6., 50.), (8., 52.), (6., 52.), (6., 50.)]
        );
    }
}
