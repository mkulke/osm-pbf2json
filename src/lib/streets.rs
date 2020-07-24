use super::geo::{Length, Midpoint, SegmentGeometry};
use super::items::AdminBoundary;
use super::items::{Segment, Street};
use itertools::Itertools;
use osmpbfreader::objects::{OsmId, OsmObj, Way};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::UnGraph;
use rayon::prelude::*;
use rstar::RTree;
use rstar::{RTreeObject, AABB};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};

const RTREE_PADDING: f64 = 0.001;

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
    pub fn id(&self) -> i64 {
        let ids: Vec<i64> = self.segments.iter().map(|segment| segment.way_id).collect();
        let mut hash = 0;
        for id in ids.iter() {
            hash ^= id;
        }
        hash
    }

    pub fn middle(&self) -> Option<(f64, f64)> {
        let geometries: Vec<&SegmentGeometry> = self
            .segments
            .iter()
            .map(|segment| &segment.geometry)
            .collect();
        geometries.midpoint()
    }

    fn boundary_matches<'a>(&self, tree: &'a RTree<AdminBoundary>) -> Vec<&'a AdminBoundary> {
        let points: Vec<[f64; 2]> = self.into();
        let aabb = AABB::from_points(&points);
        tree.locate_in_envelope_intersecting(&aabb).collect()
    }

    fn set_boundary(&mut self, name: &str) {
        self.boundary = Some(name.into());
    }

    pub fn split_by_boundaries(mut self, tree: &RTree<AdminBoundary>) -> Vec<Self> {
        let matches = self.boundary_matches(tree);
        match matches.len() {
            0 => vec![self],
            1 => {
                let boundary = matches[0];
                self.set_boundary(&boundary.name);
                return vec![self];
            }
            _ => matches
                .iter()
                .map(|boundary| {
                    let mut new_street = self.clone();
                    new_street.set_boundary(&boundary.name);
                    new_street
                })
                .collect(),
        }
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

fn get_name_groups(objs: &BTreeMap<OsmId, OsmObj>) -> HashMap<&str, Vec<&Way>> {
    objs.values()
        .filter_map(|obj| {
            let way = obj.way()?;
            let name: &str = way.tags.get("name")?;
            Some((name, way))
        })
        .into_group_map()
}

pub fn extract_streets(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<Street> {
    get_name_groups(objs)
        .into_par_iter()
        .flat_map(|(name, ways)| {
            let segments = get_segments(&ways, objs);
            let clusters = get_clusters(segments);
            let streets: Vec<Street> = clusters
                .iter()
                .map(|segments| Street {
                    name: (*name).into(),
                    segments: segments.to_vec(),
                    boundary: None,
                })
                .collect();
            streets
        })
        .collect()
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

impl From<&Street> for Vec<[f64; 2]> {
    fn from(street: &Street) -> Self {
        street
            .segments
            .iter()
            .flat_map(|segment| {
                let tuples: Vec<(f64, f64)> = segment.geometry.clone().into();
                let coordinates: Vec<[f64; 2]> = tuples
                    .iter()
                    .map(|coordinate| [coordinate.0, coordinate.1])
                    .collect();
                coordinates
            })
            .collect()
    }
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
        let way_id = way.id.0;
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
    use approx::*;
    use osmpbfreader::objects::{Node, NodeId, Tags, Way, WayId};
    use std::collections::BTreeMap;

    fn add_way(id: WayId, name: &str, nodes: Vec<NodeId>, objs: &mut BTreeMap<OsmId, OsmObj>) {
        let mut tags = Tags::new();
        tags.insert("name".into(), name.into());
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

        let streets = extract_streets(&objs);
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

        let streets = extract_streets(&objs);
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

        let streets = extract_streets(&objs);
        assert_eq!(streets.len(), 2);
    }

    fn create_segment(way_id: i64, coordinates: Vec<(f64, f64)>) -> Segment {
        let geometry = SegmentGeometry::new(coordinates).unwrap();
        Segment { way_id, geometry }
    }

    #[test]
    fn street_length() {
        let seg_1 = create_segment(42, vec![(0., 1.), (0., 3.)]);
        let seg_2 = create_segment(43, vec![(0., 3.), (1., 4.)]);
        let segments = vec![seg_1, seg_2];
        let name = String::from("some name");
        let street = Street {
            name,
            segments,
            boundary: None,
        };
        let length = street.length();
        assert_relative_eq!(length, 2.0 + 2.0_f64.sqrt(), epsilon = f64::EPSILON);
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
