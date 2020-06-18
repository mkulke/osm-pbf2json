use super::chainable::Chainable;
use super::geo::Centerable;
use geo::algorithm::bounding_rect::BoundingRect;
use geo::algorithm::intersects::Intersects;
use geo_types::LineString;
use itertools::Itertools;
use osmpbfreader::objects::{OsmId, OsmObj, Way, WayId};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::UnGraph;
use rand::random;
use rstar::RTree;
use rstar::{RTreeObject, AABB};
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::error::Error;
use std::hash::{Hash, Hasher};
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

fn get_line_string(way: &Way, objs: &BTreeMap<OsmId, OsmObj>) -> Option<LineString<f64>> {
    let coordinates = way.nodes.iter().filter_map(|&node_id| {
        let obj = objs.get(&node_id.into())?;
        let node = obj.node()?;
        let coordinate = (node.lon(), node.lat());
        Some(coordinate)
    });
    Some(coordinates.into_iter().collect())
}

fn get_segments(ways: &Vec<&Way>, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<Segment> {
    ways.iter()
        .filter_map(|way| Segment::new(way, objs).ok())
        .collect()
}

fn get_intersections<'a>(tree: &RTree<&'a Segment>) -> HashSet<(&'a Segment, &'a Segment)> {
    let mut intersections = HashSet::new();
    for segment in tree.iter() {
        let envelope = segment.envelope();
        let intersecting_segments = tree.locate_in_envelope_intersecting(&envelope);
        for other_segment in intersecting_segments {
            if !segment.line_string.intersects(&other_segment.line_string) {
                continue;
            }
            let tuple = if segment.way_id < other_segment.way_id {
                (*segment, *other_segment)
            } else {
                (*other_segment, *segment)
            };
            intersections.insert(tuple);
        }
    }
    intersections
}

fn get_clusters(segments: &Vec<Segment>) -> Vec<Vec<&Segment>> {
    let tree = RTree::<&Segment>::bulk_load(segments.iter().collect());
    let mut graph = UnGraph::<&Segment, ()>::new_undirected();

    let mut segment_idx_map: HashMap<&Segment, _> = HashMap::new();
    for segment in tree.iter() {
        let idx = graph.add_node(segment);
        segment_idx_map.insert(segment, idx);
    }

    let intersections = get_intersections(&tree);
    for intersection in intersections.iter() {
        let idx_a = segment_idx_map[intersection.0];
        let idx_b = segment_idx_map[intersection.1];
        println!("Add graph {:?} -> {:?}", idx_a, idx_b);
        graph.add_edge(idx_a, idx_b, ());
    }

    kosaraju_scc(&graph)
        .iter()
        .map(|ids| ids.iter().map(|id| graph[*id]).collect())
        .collect()
}

pub fn get_streets(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<Street> {
    let name_groups = get_name_groups(objs);
    // let x: Vec<Street> = name_groups
    //     .iter()
    //     .flat_map(|(name, ways)| {
    for (name, ways) in name_groups.iter() {
        let segments = get_segments(ways, objs);
        let clusters = get_clusters(&segments);
    }
    // clusters.into_iter().map(|cluster| Street {
    //     name: *name.clone(),
    //     segments: cluster,
    // })
    // })
    // .collect();
    unimplemented!();
}

#[derive(Debug)]
pub struct Street {
    name: String,
    segments: Vec<Segment>,
}

#[derive(Debug)]
struct Segment {
    way_id: WayId,
    bounding_box: BoundingBox,
    line_string: LineString<f64>,
}

impl Hash for Segment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.way_id.hash(state);
    }
}

impl PartialEq for Segment {
    fn eq(&self, other: &Self) -> bool {
        self.way_id == other.way_id
    }
}

impl Eq for Segment {}

impl Segment {
    fn new(way: &Way, objs: &BTreeMap<OsmId, OsmObj>) -> Result<Self, &'static str> {
        let way_id = way.id;
        let line_string =
            get_line_string(way, objs).ok_or("could not construct line string for way")?;
        let bounding_box: BoundingBox = (&line_string).try_into()?;
        let segment = Segment {
            way_id,
            line_string,
            bounding_box,
        };
        Ok(segment)
    }
}

impl TryFrom<&LineString<f64>> for BoundingBox {
    type Error = &'static str;

    fn try_from(line_string: &LineString<f64>) -> Result<Self, Self::Error> {
        line_string
            .bounding_rect()
            .map(|rect| {
                let sw = [rect.min().x, rect.min().y];
                let ne = [rect.max().x, rect.max().y];
                BoundingBox { sw, ne }
            })
            .ok_or("cannot get bounding box for the given set of coordinates")
    }
}

#[derive(Debug)]
struct BoundingBox {
    sw: [f64; 2],
    ne: [f64; 2],
}

// impl RTreeObject for Segment {
//     type Envelope = AABB<[f64; 2]>;

//     fn envelope(&self) -> Self::Envelope {
//         AABB::from_corners(self.bounding_box.sw, self.bounding_box.ne)
//     }
// }

impl RTreeObject for &Segment {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(self.bounding_box.sw, self.bounding_box.ne)
    }
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

#[cfg(test)]
mod get_streets {
    use super::*;
    use osmpbfreader::objects::WayId;

    fn create_segment(id: i64, coordinates: Vec<(f64, f64)>) -> Segment {
        let line_string: LineString<f64> = coordinates.into();
        let bounding_box: BoundingBox = (&line_string).try_into().unwrap();
        let way_id = WayId(id);
        Segment {
            way_id,
            line_string,
            bounding_box,
        }
    }

    #[test]
    fn no_intersections() {
        let seg_1 = create_segment(42, vec![(1., 1.), (1., 2.)]);
        let seg_2 = create_segment(43, vec![(3., 3.), (3., 4.)]);
        let segments = vec![&seg_1, &seg_2];
        let tree = RTree::<&Segment>::bulk_load(segments);
        let intersections = get_intersections(&tree);
        assert!(intersections.is_empty());
    }

    #[test]
    fn intersection_by_touching() {
        let seg_1 = create_segment(42, vec![(1., 1.), (1., 2.)]);
        let seg_2 = create_segment(43, vec![(3., 3.), (3., 4.)]);
        let seg_3 = create_segment(44, vec![(3., 4.), (4., 4.)]);
        let segments = vec![&seg_1, &seg_2, &seg_3];
        let tree = RTree::<&Segment>::bulk_load(segments);
        let intersections = get_intersections(&tree);
        assert_eq!(intersections.len(), 1);
        let (seg_a, seg_b) = intersections.iter().nth(0).unwrap();
        assert_eq!(*seg_a, &seg_2);
        assert_eq!(*seg_b, &seg_3);
    }

    #[test]
    fn intersection_only_by_bounding_box() {
        let seg_1 = create_segment(42, vec![(1., 1.), (3., 3.)]);
        let seg_2 = create_segment(43, vec![(2., 0.), (3., 2.)]);
        let segments = vec![&seg_1, &seg_2];
        let tree = RTree::<&Segment>::bulk_load(segments);
        let intersections = get_intersections(&tree);
        assert_eq!(intersections.len(), 0);
    }

    #[test]
    fn cluster_unrelated() {
        let seg_1 = create_segment(42, vec![(0., 1.), (0., 3.)]);
        let seg_2 = create_segment(43, vec![(1., 1.), (1., 3.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(&segments);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn cluster_crossing() {
        let seg_1 = create_segment(42, vec![(0., 1.), (3., 1.)]);
        let seg_2 = create_segment(43, vec![(2., 0.), (2., 3.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(&segments);
        assert_eq!(clusters.len(), 1);
        let cluster = &clusters[0];
        assert_eq!(cluster.len(), 2);
    }

    #[test]
    fn cluster_touching() {
        let seg_1 = create_segment(42, vec![(0., 1.), (3., 1.)]);
        let seg_2 = create_segment(43, vec![(3., 1.), (2., 3.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(&segments);
        assert_eq!(clusters.len(), 1);
        let cluster = &clusters[0];
        assert_eq!(cluster.len(), 2);
    }

    #[test]
    fn cluster_not_touching() {
        let seg_1 = create_segment(42, vec![(1., 1.), (3., 3.)]);
        let seg_2 = create_segment(43, vec![(2., 0.), (3., 2.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(&segments);
        assert_eq!(clusters.len(), 2);
    }
}
