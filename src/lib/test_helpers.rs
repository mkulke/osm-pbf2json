use osm_boundaries_utils::osm_builder::{named_node, OsmBuilder};
use osmpbfreader::objects::{OsmId, OsmObj, Relation};
use std::collections::BTreeMap;

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

#[allow(dead_code)]
pub fn create_objects(tags: &[(&str, &str)], coordinates: &[[f64; 2]]) -> BTreeMap<OsmId, OsmObj> {
    let mut nodes: Vec<_> = coordinates
        .iter()
        .enumerate()
        .map(|(idx, co)| {
            if idx == 0 {
                named_node(co[0], co[1], "start")
            } else {
                let role = idx.to_string();
                named_node(co[0], co[1], Box::leak(role.into_boxed_str()))
            }
        })
        .collect();
    let first = nodes[0].clone();
    nodes.push(first);

    let mut builder = OsmBuilder::new();
    let rel_id = builder.relation().outer(nodes).relation_id.into();
    let obj = builder.objects.get_mut(&rel_id).unwrap();
    let rel = obj.relation_mut().unwrap();
    for (key, value) in tags {
        rel.tags.insert((*key).into(), (*value).into());
    }
    builder.objects
}
