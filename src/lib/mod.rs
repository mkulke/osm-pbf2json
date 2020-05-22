use self::geo::{get_compound_coordinates, get_geo_info, Bounds, Location};
use filter::{filter, parse, Group};
use itertools::Itertools;
use osmpbfreader::objects::{Node, NodeId, OsmId, OsmObj, Relation, Tags, Way};
use osmpbfreader::OsmPbfReader;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::io::{Read, Seek, Write};

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

struct Road<'a> {
    name: &'a String,
    head_id: NodeId,
    tail_id: NodeId,
    ways: Vec<&'a Way>,
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
}

impl<'a> Road<'a> {
    fn is_tail(&self, id: NodeId, name: &str) -> bool {
        id == self.tail_id && name == self.name
    }

    fn is_head(&self, id: NodeId, name: &str) -> bool {
        id == self.head_id && name == self.name
    }

    fn to_geojson(
        &self,
        objs: &BTreeMap<OsmId, OsmObj>,
    ) -> Result<String, serde_json::error::Error> {
        let coordinates: Vec<(f64, f64)> = self
            .ways
            .iter()
            .flat_map(|way| &way.nodes)
            .dedup()
            .filter_map(|&node_id| {
                let obj = objs.get(&node_id.into())?;
                let node = obj.node()?;
                Some(node)
            })
            .map(|node| (node.lon(), node.lat()))
            .collect();
        let geometry = Geometry::LineString { coordinates };
        let entity = Entity::Feature {
            geometry,
            properties: vec![("name".to_string(), self.name.clone())]
                .into_iter()
                .collect(),
        };

        to_string(&entity)
    }
}

fn get_named_way(obj: &OsmObj) -> Option<(&Way, &String)> {
    let way = obj.way()?;
    let name = way.tags.get("name")?;
    Some((way, name))
}

fn get_roads<'a>(objs: &'a BTreeMap<OsmId, OsmObj>) -> Vec<Road<'a>> {
    let mut roads: Vec<Road> = vec![];
    for obj in objs.values() {
        // if let Some(way) = obj.way() {
        if let Some((way, name)) = get_named_way(obj) {
            let len = way.nodes.len();
            if len < 1 {
                continue;
            }
            let head_id = way.nodes[0];
            let tail_id = way.nodes[len - 1];

            // does the head node fit to the tail of a segment?
            if let Some(road) = roads.iter_mut().find(|road| road.is_tail(head_id, &name)) {
                road.ways.push(way);
                road.tail_id = tail_id;
                continue;
            }

            // does the tail node fit to the head of a segment?
            if let Some(road) = roads.iter_mut().find(|road| road.is_head(tail_id, &name)) {
                road.ways.insert(0, way);
                road.head_id = head_id;
                continue;
            }

            // otherwise add new road
            let ways = vec![way];
            let road = Road {
                name,
                head_id,
                tail_id,
                ways,
            };

            roads.push(road);
        }
    }

    roads
}

pub fn extract_streets(
    file: impl Seek + Read,
    _writer: &mut dyn Write,
) -> Result<(), Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let groups = parse("highway~primary+name+postal_code".to_string());
    let objs = pbf.get_objs_and_deps(|obj| filter(obj, &groups))?;

    let ways: Vec<&Way> = objs.values().filter_map(|obj| obj.way()).collect();
    println!("{} ways found", ways.len());

    let roads = get_roads(&objs);
    for road in roads {
        if road.name == "Alexanderstraße" {
            // println!("road {} found with {} ways", road.name, road.ways.len());
            println!("{}", road.to_geojson(&objs)?);
        }
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
mod get_roads {
    use super::*;
    use osmpbfreader::objects::{NodeId, Tags, Way, WayId};
    use std::collections::BTreeMap;

    fn create_named_way(id: i64, name: &str, nodes: Vec<NodeId>) -> Way {
        let mut tags = Tags::new();
        tags.insert("name".to_string(), name.to_string());
        Way {
            id: WayId(id),
            tags,
            nodes,
        }
    }

    #[test]
    fn one_road_with_two_segments() {
        let nodes = vec![NodeId(1), NodeId(2)];
        let way_1 = create_named_way(42, "street a", nodes);

        let nodes = vec![NodeId(2), NodeId(3)];
        let way_2 = create_named_way(41, "street a", nodes);

        let mut objs: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        objs.insert(WayId(41).into(), way_2.into());
        objs.insert(WayId(42).into(), way_1.into());

        let roads = get_roads(&objs);
        assert_eq!(roads.len(), 1);

        let road = &roads[0];
        let numbers: Vec<i64> = road.ways.iter().map(|way| way.id.0).collect();
        assert_eq!(numbers, vec![42, 41]);
    }

    #[test]
    fn connected_ways_with_distinct_names() {
        let nodes = vec![NodeId(1), NodeId(2)];
        let way_1 = create_named_way(42, "street a", nodes);

        let nodes = vec![NodeId(2), NodeId(3)];
        let way_2 = create_named_way(41, "street b", nodes);

        let mut objs: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        objs.insert(WayId(41).into(), way_2.into());
        objs.insert(WayId(42).into(), way_1.into());

        let roads = get_roads(&objs);
        assert_eq!(roads.len(), 2);
    }

    #[test]
    fn two_road_with_one_segment_each() {
        let nodes = vec![NodeId(1), NodeId(2)];
        let way_1 = create_named_way(42, "street a", nodes);

        let nodes = vec![NodeId(3), NodeId(4)];
        let way_2 = create_named_way(41, "street a", nodes);

        let mut objs: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        objs.insert(WayId(41).into(), way_2.into());
        objs.insert(WayId(42).into(), way_1.into());

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
