use self::geo::{get_compound_coordinates, get_geo_info, Bounds, Location};
use chainable::Chainable;
use filter::{filter, Condition, Group};
use itertools::Itertools;
use osmpbfreader::objects::{Node, OsmId, OsmObj, Relation, Tags, Way};
use osmpbfreader::OsmPbfReader;
use rand::random;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::io::{Read, Seek, Write};

pub mod chainable;
pub mod filter;
mod geo;

#[derive(Serialize, Deserialize)]
struct JSONStreetSegment {
    postal_code: String,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Serialize, Deserialize)]
struct JSONStreet {
    name: String,
    segments: Vec<JSONStreetSegment>,
}

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

#[derive(Debug)]
struct Road {
    name: String,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Geometry {
    LineString { coordinates: Vec<(f64, f64)> },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Entity {
    Feature {
        properties: HashMap<String, String>,
        geometry: Geometry,
    },
    FeatureCollection {
        features: Vec<Entity>,
    },
}

trait GeojsonExt {
    fn to_geojson(&self) -> Result<String, Box<dyn Error>>;
}

impl GeojsonExt for Vec<Road> {
    fn to_geojson(&self) -> Result<String, Box<dyn Error>> {
        let features = self
            .iter()
            .filter(|road| road.coordinates.len() >= 2)
            .map(|road| {
                let coordinates = road.coordinates.clone();
                let geometry = Geometry::LineString { coordinates };
                let r = random::<u8>();
                let g = random::<u8>();
                let b = random::<u8>();
                let random_color = format!("#{:02X}{:02X}{:02X}", r, g, b);
                Entity::Feature {
                    geometry,
                    properties: vec![
                        ("name".to_string(), road.name.clone()),
                        ("stroke".to_string(), random_color),
                    ]
                    .into_iter()
                    .collect(),
                }
            })
            .collect();

        let feature_collection = Entity::FeatureCollection { features };
        let string = to_string(&feature_collection)?;
        Ok(string)
    }
}

fn get_named_way(obj: &OsmObj) -> Option<(&Way, String)> {
    let way = obj.way()?;
    let name = way.tags.get("name")?;
    Some((way, name.to_owned()))
}

fn get_road_segment(obj: &OsmObj, objs: &BTreeMap<OsmId, OsmObj>) -> Option<Road> {
    let (way, name) = get_named_way(obj)?;
    let coordinates: Vec<(f64, f64)> = way
        .nodes
        .iter()
        .filter_map(|&node_id| {
            let obj = objs.get(&node_id.into())?;
            let node = obj.node()?;
            let coordinate = (node.lon(), node.lat());
            Some(coordinate)
        })
        .collect();

    let road = Road { name, coordinates };
    Some(road)
}

fn get_roads(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<Road> {
    let name_groups = objs
        .values()
        .filter_map(|obj| {
            let way = obj.way()?;
            let name = way.tags.get("name")?;
            Some((name, obj))
        })
        .into_group_map();

    let mut roads: Vec<Road> = vec![];
    for (name, group) in name_groups.into_iter() {
        // println!("name group: {}", name);
        let mut nested_coordinates: Vec<Vec<(f64, f64)>> = group
            .iter()
            .filter_map(|obj| {
                let road = get_road_segment(obj, objs)?;
                Some(road.coordinates)
            })
            .collect();
        nested_coordinates.merge();
        for chain in nested_coordinates {
            let chained_road = Road {
                name: name.to_owned(),
                coordinates: chain,
            };
            roads.push(chained_road);
        }
    }

    roads
}

fn build_street_group() -> Vec<Group> {
    let values = vec!["primary", "secondary", "tertiary", "residential", "service"];

    let groups = values
        .iter()
        .map(|val| {
            let highway_match = Condition::ValueMatch("highway".to_string(), val.to_string());
            let name_presence = Condition::TagPresence("name".to_string());
            let conditions = vec![highway_match, name_presence];
            Group { conditions }
        })
        .collect();
    groups
}

pub fn extract_streets(
    file: impl Seek + Read,
    writer: &mut dyn Write,
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let groups = build_street_group();
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, &groups))?;

    let roads = get_roads(&objs);
    let geojson = roads.to_geojson()?;
    writeln!(writer, "{}", geojson)?;

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
mod roads {
    use super::*;

    macro_rules! road {
        ($($x: expr), *) => {
            Road {
                name: "abc street".to_string(),
                coordinates: vec![$($x), *],
            }
        };
    }

    macro_rules! coordinates {
        ($($x: expr), *) => {
            vec![$($x.coordinates), *]
        };
    }

    #[test]
    fn merge_tail() {
        let road_1 = road![(1., 1.), (2., 2.)];
        let road_2 = road![(2., 2.), (3., 3.)];

        let mut coordinates = coordinates![road_1, road_2];
        coordinates.merge();
        assert_eq!(coordinates, vec![vec![(1., 1.), (2., 2.), (3., 3.)]]);
    }

    #[test]
    fn merge_disjointed() {
        let road_1 = road![(3., 3.), (4., 4.)];
        let road_2 = road![(1., 1.), (2., 2.)];
        let road_3 = road![(2., 2.), (3., 3.)];

        let mut coordinates = coordinates![road_1, road_2, road_3];
        coordinates.merge();
        assert_eq!(
            coordinates,
            vec![vec![(1., 1.), (2., 2.), (3., 3.), (4., 4.)]]
        );
    }

    #[test]
    fn merge_head() {
        let road_1 = road![(2., 2.), (3., 3.)];
        let road_2 = road![(1., 1.), (2., 2.)];

        let mut coordinates = coordinates![road_1, road_2];
        coordinates.merge();
        assert_eq!(coordinates, vec![vec![(1., 1.), (2., 2.), (3., 3.)]]);
    }

    #[test]
    fn merge_reverse_tail() {
        let road_1 = road![(1., 1.), (2., 2.)];
        let road_2 = road![(3., 3.), (2., 2.)];

        let mut coordinates = coordinates![road_1, road_2];
        coordinates.merge();
        assert_eq!(coordinates, vec![vec![(1., 1.), (2., 2.), (3., 3.)]]);
    }

    #[test]
    fn merge_reverse_head() {
        let road_1 = road![(2., 2.), (3., 3.)];
        let road_2 = road![(2., 2.), (1., 1.)];

        let mut coordinates = coordinates![road_1, road_2];
        coordinates.merge();
        assert_eq!(coordinates, vec![vec![(1., 1.), (2., 2.), (3., 3.)]]);
    }
}

#[cfg(test)]
mod get_roads {
    use super::*;
    use osmpbfreader::objects::{NodeId, Tags, Way, WayId};
    use std::collections::BTreeMap;

    fn add_way(id: WayId, name: &str, nodes: Vec<NodeId>, objs: &mut BTreeMap<OsmId, OsmObj>) {
        let mut tags = Tags::new();
        tags.insert("name".to_string(), name.to_string());
        let way = Way { id, tags, nodes };
        objs.insert(id.into(), way.into());
    }

    fn add_node(id: NodeId, lng: f64, lat: f64, objs: &mut BTreeMap<OsmId, OsmObj>) {
        let tags = Tags::new();
        let decimicro_lat = lat as i32 * 10_000_000;
        let decimicro_lon = lng as i32 * 10_000_000;
        let node = Node {
            id,
            tags,
            decimicro_lat,
            decimicro_lon,
        };
        objs.insert(id.into(), node.into());
    }

    #[test]
    fn one_road_with_three_segments() {
        let mut objs: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        add_node(NodeId(1), 13., 52., &mut objs);
        add_node(NodeId(2), 14., 52., &mut objs);
        add_node(NodeId(3), 14., 53., &mut objs);
        add_node(NodeId(4), 15., 53., &mut objs);

        let node_ids = vec![NodeId(1), NodeId(2)];
        add_way(WayId(42), "street a", node_ids, &mut objs);

        let node_ids = vec![NodeId(2), NodeId(3)];
        add_way(WayId(41), "street a", node_ids, &mut objs);

        let node_ids = vec![NodeId(3), NodeId(4)];
        add_way(WayId(43), "street a", node_ids, &mut objs);

        let roads = get_roads(&objs);
        assert_eq!(roads.len(), 1);

        let road = &roads[0];
        assert_eq!(
            road.coordinates,
            vec![(13.0, 52.0), (14.0, 52.0), (14.0, 53.0), (15.0, 53.0)]
        );
    }

    #[test]
    fn connected_ways_with_distinct_names() {
        let mut objs: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        add_node(NodeId(1), 13., 52., &mut objs);
        add_node(NodeId(2), 14., 52., &mut objs);
        add_node(NodeId(3), 14., 53., &mut objs);

        let node_ids = vec![NodeId(1), NodeId(2)];
        add_way(WayId(42), "street a", node_ids, &mut objs);

        let node_ids = vec![NodeId(2), NodeId(3)];
        add_way(WayId(41), "street b", node_ids, &mut objs);

        let roads = get_roads(&objs);
        assert_eq!(roads.len(), 2);
    }

    #[test]
    fn two_road_with_one_segment_each() {
        let mut objs: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        add_node(NodeId(1), 13., 52., &mut objs);
        add_node(NodeId(2), 14., 52., &mut objs);
        add_node(NodeId(3), 14., 53., &mut objs);
        add_node(NodeId(4), 15., 53., &mut objs);

        let node_ids = vec![NodeId(1), NodeId(2)];
        add_way(WayId(42), "street a", node_ids, &mut objs);

        let node_ids = vec![NodeId(2), NodeId(3)];
        add_way(WayId(41), "street b", node_ids, &mut objs);

        let roads = get_roads(&objs);
        assert_eq!(roads.len(), 2);
    }
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
