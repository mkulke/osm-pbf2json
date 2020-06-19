use super::chainable::Chainable;
use super::geo::Centerable;
use itertools::Itertools;
use osmpbfreader::objects::{OsmId, OsmObj, Way};
use rand::random;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::io::Write;

#[derive(Debug)]
pub struct Road {
    name: String,
    coordinates: Vec<(f64, f64)>,
}

#[derive(Serialize, Deserialize)]
struct JSONStreet {
    id: i64,
    name: String,
    loc: [f64; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Geometry {
    LineString { coordinates: Vec<(f64, f64)> },
    MultiLineString { coordinates: Vec<Vec<(f64, f64)>> },
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

pub trait OutputExt {
    fn to_geojson(&self) -> Result<String, Box<dyn Error>>;
    fn write_json_lines(self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>>;
}

impl OutputExt for Vec<Road> {
    fn write_json_lines(self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        for (idx, road) in self.into_iter().enumerate() {
            let id = 420_000_000 + idx as i64;
            let middle = road
                .coordinates
                .get_middle()
                .ok_or("could not calculate middle")?;
            let name = road.name;
            let loc = middle.into();
            let json_street = JSONStreet { id, name, loc };
            let json = to_string(&json_street)?;
            writeln!(writer, "{}", json)?;
        }
        Ok(())
    }

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

fn get_road_segment(name: &String, way: &Way, objs: &BTreeMap<OsmId, OsmObj>) -> Option<Road> {
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

    let name = name.clone();
    let road = Road { name, coordinates };
    Some(road)
}

fn get_name_groups(objs: &BTreeMap<OsmId, OsmObj>) -> HashMap<&String, Vec<&Way>> {
    objs.values()
        .filter_map(|obj| {
            let way = obj.way()?;
            let name = way.tags.get("name")?;
            Some((name, way))
        })
        .into_group_map()
}

pub fn get_roads(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<Road> {
    let name_groups = get_name_groups(objs);
    let mut roads: Vec<Road> = vec![];
    for (name, group) in name_groups.into_iter() {
        // println!("name group: {}", name);
        let mut nested_coordinates: Vec<Vec<(f64, f64)>> = group
            .iter()
            .filter_map(|way| {
                let road = get_road_segment(name, way, objs)?;
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

#[cfg(test)]
mod merge {
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
    use osmpbfreader::objects::{Node, NodeId, Tags, Way, WayId};
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
