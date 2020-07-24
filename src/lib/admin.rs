use super::geo::BoundaryGeometry;
use super::items::AdminBoundary;
use osm_boundaries_utils::build_boundary;
use osmpbfreader::objects::{OsmId, OsmObj};
use rstar::{RTreeObject, AABB};
use std::collections::BTreeMap;

impl RTreeObject for AdminBoundary {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let (sw, ne) = self.geometry.sw_ne();
        AABB::from_corners(sw, ne)
    }
}

pub fn get_boundaries(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<AdminBoundary> {
    objs.values()
        .filter_map(|obj| {
            let relation = obj.relation()?;
            let boundary = relation.tags.get("boundary")?;
            if boundary != "administrative" {
                return None;
            }
            let name = relation.tags.get("name")?.clone().into();
            let admin_level = relation.tags.get("admin_level")?.parse().ok()?;
            let multi_polygon = build_boundary(relation, objs)?;
            let geometry = BoundaryGeometry::new(multi_polygon).ok()?;
            let boundary = AdminBoundary {
                name,
                admin_level,
                geometry,
            };
            Some(boundary)
        })
        .collect()
}

#[cfg(test)]
mod get_boundaries {
    use super::*;
    use osm_boundaries_utils::osm_builder::{named_node, OsmBuilder};
    use osmpbfreader::objects::{NodeId, OsmObj, Relation, RelationId, WayId};
    use rstar::RTree;

    trait OsmObjExt {
        fn relation_mut(&mut self) -> Option<&mut Relation>;
    }

    impl OsmObjExt for OsmObj {
        fn relation_mut(&mut self) -> Option<&mut Relation> {
            if let OsmObj::Relation(ref mut rel) = *self {
                Some(rel)
            } else {
                None
            }
        }
    }

    fn create_objects(tags: &[(&str, &str)], offset: f64) -> BTreeMap<OsmId, OsmObj> {
        let mut builder = OsmBuilder::new();
        let rel_id = builder
            .relation()
            .outer(vec![
                named_node(offset, 53., "start"),
                named_node(offset, 52., "1"),
                named_node(offset + 1., 52., "2"),
                named_node(offset + 1., 53., "3"),
                named_node(offset, 53., "start"),
            ])
            .relation_id
            .into();
        let obj = builder.objects.get_mut(&rel_id).unwrap();
        let rel = obj.relation_mut().unwrap();
        for (key, value) in tags {
            rel.tags.insert((*key).into(), (*value).into());
        }
        builder.objects
    }

    fn bump_ids(objs: BTreeMap<OsmId, OsmObj>) -> BTreeMap<OsmId, OsmObj> {
        objs.into_iter()
            .map(|(key, value)| {
                let id = key.inner_id() + 1000;
                match value {
                    OsmObj::Node(mut node) => {
                        let node_id = NodeId(id);
                        node.id = node_id;
                        (OsmId::Node(node_id), OsmObj::Node(node))
                    }
                    OsmObj::Way(mut way) => {
                        let way_id = WayId(id);
                        way.id = way_id;
                        let node_ids = way
                            .nodes
                            .iter()
                            .map(|node_id| NodeId(node_id.0 + 1000))
                            .collect();
                        way.nodes = node_ids;
                        (OsmId::Way(way_id), OsmObj::Way(way))
                    }
                    OsmObj::Relation(mut relation) => {
                        let relation_id = RelationId(id);
                        for a_ref in relation.refs.iter_mut() {
                            let ref_id = a_ref.member.inner_id() + 1000;
                            a_ref.member = OsmId::Way(WayId(ref_id));
                        }
                        (OsmId::Relation(relation_id), OsmObj::Relation(relation))
                    }
                }
            })
            .collect()
    }

    #[test]
    fn geometry() {
        let tags = vec![
            ("boundary", "administrative"),
            ("name", "some_name"),
            ("admin_level", "11"),
        ];
        let objects = create_objects(&tags, 13.);

        let boundary = get_boundaries(&objects).pop().unwrap();
        let coordinates = boundary.geometry.coordinates();
        assert_eq!(coordinates.len(), 1);
        assert_eq!(coordinates[0].len(), 1);
        assert_eq!(coordinates[0][0].len(), 5);
    }

    #[test]
    fn boundary_with_multiple_nodes() {
        let tags = vec![
            ("boundary", "administrative"),
            ("name", "some_name"),
            ("admin_level", "11"),
        ];
        let objects = create_objects(&tags, 13.);
        let boundaries = get_boundaries(&objects);
        assert_eq!(boundaries.len(), 1);
    }

    #[test]
    fn relation_with_wrong_tags() {
        let tags = vec![
            ("boundary", "wrong"),
            ("name", "some_name"),
            ("admin_level", "11"),
        ];
        let objects = create_objects(&tags, 13.);
        let boundaries = get_boundaries(&objects);
        assert_eq!(boundaries.len(), 0);
    }

    #[test]
    fn locate_line_string_contained_in_boundary() {
        let tags = vec![
            ("boundary", "administrative"),
            ("name", "some_name"),
            ("admin_level", "11"),
        ];
        let objects = create_objects(&tags, 13.);
        let boundaries = get_boundaries(&objects);
        let tree = RTree::<AdminBoundary>::bulk_load(boundaries);
        let aabb = AABB::from_points(&vec![[13.25, 52.5], [13.74, 52.5]]);
        let matches = tree.locate_in_envelope_intersecting(&aabb);
        assert_eq!(matches.count(), 1);
    }

    #[test]
    fn locate_line_string_intersecting_boundary() {
        let tags = vec![
            ("boundary", "administrative"),
            ("name", "some_name"),
            ("admin_level", "11"),
        ];
        let objects = create_objects(&tags, 13.);
        let boundaries = get_boundaries(&objects);
        let tree = RTree::<AdminBoundary>::bulk_load(boundaries);
        let aabb = AABB::from_points(&vec![[12.75, 52.5], [13.25, 52.5]]);
        let matches = tree.locate_in_envelope_intersecting(&aabb);
        assert_eq!(matches.count(), 1);
    }

    #[test]
    fn locate_line_string_out_of_boundary() {
        let tags = vec![
            ("boundary", "administrative"),
            ("name", "some_name"),
            ("admin_level", "11"),
        ];
        let objects = create_objects(&tags, 13.);
        let boundaries = get_boundaries(&objects);
        let tree = RTree::<AdminBoundary>::bulk_load(boundaries);
        let aabb = AABB::from_points(&vec![[12.25, 52.5], [12.75, 52.5]]);
        let matches = tree.locate_in_envelope_intersecting(&aabb);
        assert_eq!(matches.count(), 0);
    }

    #[test]
    fn locate_line_string_intersecting_two_boundaries() {
        let tags = vec![
            ("boundary", "administrative"),
            ("name", "some_name"),
            ("admin_level", "11"),
        ];
        let mut objects_1 = create_objects(&tags, 13.);
        let objects_2 = bump_ids(create_objects(&tags, 12.));
        objects_1.extend(objects_2);
        let boundaries = get_boundaries(&objects_1);
        assert_eq!(boundaries.len(), 2);
        let tree = RTree::<AdminBoundary>::bulk_load(boundaries);
        let aabb = AABB::from_points(&vec![[13.25, 52.5], [13.75, 52.5]]);
        let matches = tree.locate_in_envelope_intersecting(&aabb);
        assert_eq!(matches.count(), 1);

        let aabb = AABB::from_points(&vec![[12.5, 52.5], [13.5, 52.5]]);
        let matches = tree.locate_in_envelope_intersecting(&aabb);
        assert_eq!(matches.count(), 2);
    }
}
