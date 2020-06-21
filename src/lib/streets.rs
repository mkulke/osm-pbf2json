use super::geo::{Length, Midpoint, SegmentGeometry};
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
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::io::Write;

const RTREE_PADDING: f64 = 0.001;

#[derive(Serialize, Deserialize)]
struct JSONStreet {
    id: i64,
    name: String,
    length: f64,
    loc: (f64, f64),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Geometry {
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

impl OutputExt for Vec<Street> {
    fn write_json_lines(self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        for street in self.iter() {
            let id = street.id();
            let loc = street.middle().ok_or("could not calculate middle")?;
            let name = street.name.clone();
            let length = street.length();
            let json_street = JSONStreet {
                id,
                name,
                length,
                loc,
            };
            let json = to_string(&json_street)?;
            writeln!(writer, "{}", json)?;
        }
        Ok(())
    }

    fn to_geojson(&self) -> Result<String, Box<dyn Error>> {
        let features = self
            .iter()
            .filter_map(|street| {
                let geometries: Vec<_> = street
                    .segments
                    .iter()
                    .filter(|segment| segment.geometry.len() >= 2)
                    .map(|segment| segment.geometry.clone())
                    .collect();
                if geometries.is_empty() {
                    return None;
                }
                let coordinates = geometries.iter().map(|g| g.into()).collect();
                let geometry = Geometry::MultiLineString { coordinates };
                let r = random::<u8>();
                let g = random::<u8>();
                let b = random::<u8>();
                let random_color = format!("#{:02X}{:02X}{:02X}", r, g, b);
                let entity = Entity::Feature {
                    geometry,
                    properties: vec![
                        ("name".to_string(), street.name.clone()),
                        ("stroke".to_string(), random_color),
                    ]
                    .into_iter()
                    .collect(),
                };
                Some(entity)
            })
            .collect();

        let feature_collection = Entity::FeatureCollection { features };
        let string = to_string(&feature_collection)?;
        Ok(string)
    }
}

impl Length for Street {
    fn length(&self) -> f64 {
        let geometries: Vec<&SegmentGeometry> = self
            .segments
            .iter()
            .map(|segment| &segment.geometry)
            .collect();
        geometries.length()
    }
}

impl Street {
    fn id(&self) -> i64 {
        let ids: Vec<WayId> = self.segments.iter().map(|segment| segment.way_id).collect();
        let mut hash = 0;
        for id in ids.iter() {
            hash ^= id.0;
        }
        hash
    }

    fn middle(&self) -> Option<(f64, f64)> {
        let geometries: Vec<&SegmentGeometry> = self
            .segments
            .iter()
            .map(|segment| &segment.geometry)
            .collect();
        geometries.midpoint()
    }
}

fn get_coordinates(way: &Way, objs: &BTreeMap<OsmId, OsmObj>) -> Option<Vec<(f64, f64)>> {
    let coordinates = way
        .nodes
        .iter()
        .filter_map(|&node_id| {
            let obj = objs.get(&node_id.into())?;
            let node = obj.node()?;
            let coordinate = (node.lon(), node.lat());
            Some(coordinate)
        })
        .collect();
    Some(coordinates)
}

fn get_segments(ways: &[&Way], objs: &BTreeMap<OsmId, OsmObj>) -> Vec<Segment> {
    ways.iter()
        .filter_map(|way| Segment::new(way, objs).ok())
        .collect()
}

fn get_intersections(tree: &RTree<Segment>) -> HashSet<(&Segment, &Segment)> {
    let mut intersections = HashSet::new();
    for segment in tree.iter() {
        let (sw, ne) = segment.geometry.padded_sw_ne(RTREE_PADDING);
        let padded_envelope = AABB::from_corners(sw, ne);
        let intersecting_segments = tree.locate_in_envelope_intersecting(&padded_envelope);
        for other_segment in intersecting_segments {
            let tuple = if segment.way_id < other_segment.way_id {
                (segment, other_segment)
            } else {
                (other_segment, segment)
            };
            intersections.insert(tuple);
        }
    }
    intersections
}

fn get_clusters(segments: Vec<Segment>) -> Vec<Vec<Segment>> {
    let tree = RTree::<Segment>::bulk_load(segments);
    let mut graph = UnGraph::<Segment, ()>::new_undirected();

    let mut segment_idx_map: HashMap<&Segment, _> = HashMap::new();
    for segment in tree.into_iter() {
        let idx = graph.add_node(segment.clone());
        segment_idx_map.insert(segment, idx);
    }

    let intersections = get_intersections(&tree);
    for intersection in intersections.iter() {
        let idx_a = segment_idx_map[intersection.0];
        let idx_b = segment_idx_map[intersection.1];
        graph.add_edge(idx_a, idx_b, ());
    }

    kosaraju_scc(&graph)
        .iter()
        .map(|ids| ids.iter().map(|id| graph[*id].clone()).collect())
        .collect()
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

pub fn get_streets(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<Street> {
    get_name_groups(objs)
        .into_iter()
        .flat_map(|(name, ways)| {
            let segments = get_segments(&ways, objs);
            let clusters = get_clusters(segments);
            let streets: Vec<Street> = clusters
                .iter()
                .map(|segments| Street {
                    name: name.clone(),
                    segments: segments.to_vec(),
                })
                .collect();
            streets
        })
        .collect()
}

#[derive(Debug)]
pub struct Street {
    name: String,
    segments: Vec<Segment>,
}

impl From<&Street> for Vec<Vec<(f64, f64)>> {
    fn from(street: &Street) -> Self {
        street
            .segments
            .iter()
            .map(|segment| segment.geometry.clone().into())
            .collect()
    }
}

#[derive(Clone, Debug)]
struct Segment {
    way_id: WayId,
    geometry: SegmentGeometry,
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
        let coordinates =
            get_coordinates(way, objs).ok_or("could not construct coordinates for way")?;
        let geometry = SegmentGeometry::new(coordinates)?;
        let segment = Segment { way_id, geometry };
        Ok(segment)
    }
}

impl RTreeObject for Segment {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let (sw, ne) = self.geometry.sw_ne();
        AABB::from_corners(sw, ne)
    }
}

#[cfg(test)]
mod get_streets {
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
    fn one_street_with_three_segments() {
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

        let streets = get_streets(&objs);
        assert_eq!(streets.len(), 1);

        let street = &streets[0];
        let nested_coordinates: Vec<Vec<(f64, f64)>> = street.into();
        assert_eq!(
            nested_coordinates,
            vec![
                vec![(14.0, 53.0), (15.0, 53.0)],
                vec![(14.0, 52.0), (14.0, 53.0)],
                vec![(13.0, 52.0), (14.0, 52.0)],
            ]
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

        let streets = get_streets(&objs);
        assert_eq!(streets.len(), 2);
    }

    #[test]
    fn two_streets_with_one_segment_each() {
        let mut objs: BTreeMap<OsmId, OsmObj> = BTreeMap::new();
        add_node(NodeId(1), 13., 52., &mut objs);
        add_node(NodeId(2), 14., 52., &mut objs);
        add_node(NodeId(3), 14., 53., &mut objs);
        add_node(NodeId(4), 15., 53., &mut objs);

        let node_ids = vec![NodeId(1), NodeId(2)];
        add_way(WayId(42), "street a", node_ids, &mut objs);

        let node_ids = vec![NodeId(2), NodeId(3)];
        add_way(WayId(41), "street b", node_ids, &mut objs);

        let streets = get_streets(&objs);
        assert_eq!(streets.len(), 2);
    }

    fn create_segment(id: i64, coordinates: Vec<(f64, f64)>) -> Segment {
        let way_id = WayId(id);
        let geometry = SegmentGeometry::new(coordinates).unwrap();
        Segment { way_id, geometry }
    }

    #[test]
    fn cluster_unrelated() {
        let seg_1 = create_segment(42, vec![(0., 1.), (0., 3.)]);
        let seg_2 = create_segment(43, vec![(1., 1.), (1., 3.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(segments);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn cluster_crossing() {
        let seg_1 = create_segment(42, vec![(0., 1.), (3., 1.)]);
        let seg_2 = create_segment(43, vec![(2., 0.), (2., 3.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(segments);
        assert_eq!(clusters.len(), 1);
        let cluster = &clusters[0];
        assert_eq!(cluster.len(), 2);
    }

    #[test]
    fn cluster_touching() {
        let seg_1 = create_segment(42, vec![(0., 1.), (3., 1.)]);
        let seg_2 = create_segment(43, vec![(3., 1.), (2., 3.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(segments);
        assert_eq!(clusters.len(), 1);
        let cluster = &clusters[0];
        assert_eq!(cluster.len(), 2);
    }

    #[test]
    fn cluster_not_touching_but_overlapping_bbox() {
        let seg_1 = create_segment(42, vec![(1., 1.), (3., 3.)]);
        let seg_2 = create_segment(43, vec![(2., 0.), (3., 2.)]);
        let segments = vec![seg_1, seg_2];
        let clusters = get_clusters(segments);
        assert_eq!(clusters.len(), 1);
    }
}
