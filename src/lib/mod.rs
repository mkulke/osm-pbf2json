//! A parser/filter for OSM protobuf bundles.

use self::geo::get_compound_coordinates;
use self::items::{osm, AdminBoundary, Street};
use admin::get_boundaries;
use filter::{Condition, Filter, Group};
use osmpbfreader::objects::{OsmId, OsmObj, Relation, RelationId, Way};
use osmpbfreader::OsmPbfReader;
use rstar::RTree;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{Read, Seek};
use streets::extract_streets;

mod admin;
pub mod filter;
mod geo;
mod geojson;
pub mod items;
pub mod output;
mod streets;
mod test_helpers;

trait OsmExt {
    fn get_coordinates(&self, objs: &BTreeMap<OsmId, OsmObj>) -> Vec<(f64, f64)>;
}

trait OsmCycle {
    fn get_coordinates(
        &self,
        objs: &BTreeMap<OsmId, OsmObj>,
        visited: &mut Vec<RelationId>,
    ) -> Vec<(f64, f64)>;
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

impl OsmCycle for Relation {
    fn get_coordinates(
        &self,
        objs: &BTreeMap<OsmId, OsmObj>,
        visited: &mut Vec<RelationId>,
    ) -> Vec<(f64, f64)> {
        if visited.contains(&self.id) {
            return vec![];
        }
        visited.push(self.id);
        let coordinates = self
            .refs
            .iter()
            .filter_map(|osm_ref| {
                let obj = objs.get(&osm_ref.member)?;
                let coordinates = match obj {
                    OsmObj::Node(node) => vec![(node.lon(), node.lat())],
                    OsmObj::Way(way) => way.get_coordinates(objs),
                    OsmObj::Relation(rel) => rel.get_coordinates(objs, visited),
                };
                Some(coordinates)
            })
            .flatten()
            .collect();
        get_compound_coordinates(coordinates)
    }
}

fn build_admin_group(levels: Vec<u8>) -> Vec<Group> {
    levels
        .iter()
        .map(|level| {
            let level_match = Condition::new("admin_level", Some(&level.to_string()));
            let boundary_match = Condition::new("boundary", Some("administrative"));
            let conditions = vec![boundary_match, level_match];
            Group { conditions }
        })
        .collect()
}

fn build_street_group(name: Option<&str>) -> Vec<Group> {
    let values = vec![
        "primary",
        "secondary",
        "tertiary",
        "residential",
        "service",
        "living_street",
        "pedestrian",
    ];

    let name_condition = Condition::new("name", name);
    values
        .into_iter()
        .map(|val| {
            let highway_match = Condition::new("highway", Some(val));
            let conditions = vec![highway_match, name_condition.clone()];
            Group { conditions }
        })
        .collect()
}

/// Extract administrative boundaries from OSM
///
/// Administrative boundaries are stored in OSM as Relations with the Tag `boundary: administrative` and a `admin_level`. The meaning of the individual levels (state, country, etc.) depends on the respective region (read [here](https://wiki.openstreetmap.org/wiki/Key:admin_level) for details).
///
/// The levels can be specified, by default `4, 6, 8, 9, 10` are considered.
///
/// # Example
///
/// ```
/// use std::fs::File;
/// use osm_pbf2json::boundaries;
///
/// let file = File::open("./tests/data/wilhelmstrasse.pbf").unwrap();
/// let boundaries = boundaries(file, Some(vec![10])).unwrap();
/// assert_eq!(boundaries.len(), 2);
/// ```
pub fn boundaries(
    file: impl Seek + Read,
    levels: Option<Vec<u8>>,
) -> Result<Vec<AdminBoundary>, Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let default_levels = vec![4, 6, 8, 9, 10];
    let levels = levels.unwrap_or(default_levels);
    let groups = build_admin_group(levels);
    let objs = pbf.get_objs_and_deps(|obj| obj.filter(&groups))?;
    let boundaries = get_boundaries(&objs);
    Ok(boundaries)
}

/// Extract a list of streets from a set of OSM Objects
///
/// Streets are represented in OSM as a collection of smaller Way segments. To cluster those into distinct street entities `name` Tag and the geographical distance are considered.
///
/// A `name` can be given to retrieve only streets with a matching name.
///
/// Sometimes continuous streets cross boundaries. Streets are split along administrative boundary borders, when specifying a `boundary` option.
///
/// # Example
///
/// ```
/// use std::fs::File;
/// use osm_pbf2json::streets;
///
/// let file = File::open("./tests/data/wilhelmstrasse.pbf").unwrap();
/// let name = "Wilhelmstraße";
/// let streets = streets(file, Some(name), Some(10)).unwrap();
/// assert_eq!(streets.len(), 2);
/// ```
pub fn streets(
    file: impl Seek + Read,
    name: Option<&str>,
    boundary: Option<u8>,
) -> Result<Vec<Street>, Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);
    let groups = build_street_group(name);
    let objs = pbf.get_objs_and_deps(|obj| obj.filter(&groups))?;
    let streets = extract_streets(&objs);
    let streets = {
        match boundary {
            None => streets,
            Some(level) => {
                let groups = build_admin_group(vec![level]);
                let objs = pbf.get_objs_and_deps(|obj| obj.filter(&groups))?;
                let boundaries = get_boundaries(&objs);
                let tree = RTree::<AdminBoundary>::bulk_load(boundaries);
                streets
                    .into_iter()
                    .flat_map(|street| street.split_by_boundaries(&tree))
                    .collect()
            }
        }
    };
    Ok(streets)
}

/// Extract Objects from OSM
///
/// Objects (i.e. Nodes, Ways & Relations) will be extracted according to filter options. Some geographic properties (centroid, bounding boxes) are computed for all entities.
///
/// Filtering `groups` can be applied to select objects according to their tags.
///
/// # Example
///
/// ```
/// use std::fs::File;
/// use osm_pbf2json::objects;
/// use osm_pbf2json::filter::{Condition, Group};
///
/// let file = File::open("./tests/data/alexanderplatz.pbf").unwrap();
/// let cond_1 = Condition::new("surface", Some("cobblestone"));
/// let cond_2 = Condition::new("highway", None);
/// let group = Group { conditions: vec![cond_1, cond_2] };
/// let cobblestone_ways = objects(file, Some(&vec![group]), false).unwrap();
/// assert_eq!(cobblestone_ways.len(), 4);
/// ```
pub fn objects(
    file: impl Seek + Read,
    groups: Option<&[Group]>,
    retain_coordinates: bool,
) -> Result<Vec<osm::Object>, Box<dyn Error>> {
    let mut pbf = OsmPbfReader::new(file);

    let objs = match groups {
        Some(grps) => pbf.get_objs_and_deps(|obj| obj.filter(grps))?,
        None => pbf.get_objs_and_deps(|_| true)?,
    };

    let objects = objs
        .values()
        .filter_map(|obj| {
            if groups.is_some() && !obj.filter(groups?) {
                return None;
            }

            let object = match obj {
                OsmObj::Node(obj) => {
                    let geo_info = osm::GeoInfo::Point {
                        lon: obj.lon(),
                        lat: obj.lat(),
                    };
                    osm::Object::new(obj.id.0, "node", obj.tags.clone(), geo_info)
                }
                OsmObj::Way(obj) => {
                    let coordinates = obj.get_coordinates(&objs);
                    let geo_info = osm::GeoInfo::new_shape(&coordinates, retain_coordinates);
                    osm::Object::new(obj.id.0, "way", obj.tags.clone(), geo_info)
                }
                OsmObj::Relation(obj) => {
                    let coordinates = obj.get_coordinates(&objs, &mut vec![]);
                    let geo_info = osm::GeoInfo::new_shape(&coordinates, retain_coordinates);
                    osm::Object::new(obj.id.0, "relation", obj.tags.clone(), geo_info)
                }
            };
            Some(object)
        })
        .collect();
    Ok(objects)
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
                role: "something".into(),
            })
            .collect()
    }

    #[test]
    fn relation_without_refs() {
        let obj_map = BTreeMap::new();
        let id = RelationId(42);
        let rel = create_relation(id, vec![]);
        let coordinates = rel.get_coordinates(&obj_map, &mut vec![]);
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

        let coordinates = rel.get_coordinates(&obj_map, &mut vec![]);
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
        let coordinates = rel.get_coordinates(&obj_map, &mut vec![]);
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
        let coordinates = rel.get_coordinates(&obj_map, &mut vec![]);

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

        let coordinates = parent_rel.get_coordinates(&obj_map, &mut vec![]);

        assert_eq!(
            coordinates,
            vec![(6., 50.), (8., 52.), (6., 52.), (6., 50.)]
        );
    }

    #[test]
    fn nested_relations_with_cycle() {
        let mut obj_map = BTreeMap::new();
        let rel_id_1 = RelationId(42);
        let rel_id_2 = RelationId(44);
        let refs = create_refs(vec![rel_id_2.into()]);
        let rel_1 = create_relation(rel_id_1, refs);
        obj_map.insert(rel_id_1.into(), rel_1.into());

        let node_id = NodeId(43);
        let node = create_node(node_id, 8, 52);
        obj_map.insert(node_id.into(), node.into());

        let refs = create_refs(vec![rel_id_1.into(), node_id.into()]);
        let rel_2 = create_relation(rel_id_2, refs);

        let coordinates = rel_2.get_coordinates(&obj_map, &mut vec![]);

        assert_eq!(coordinates, vec![(8., 52.)]);
    }
}
