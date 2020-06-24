use geo::algorithm::bounding_rect::BoundingRect;
use geo_types::MultiPolygon;
use osm_boundaries_utils::build_boundary;
use osmpbfreader::objects::{OsmId, OsmObj};
use rstar::{RTreeObject, AABB};
use std::collections::BTreeMap;

pub struct AdminBoundary {
    name: String,
    admin_level: u8,
    geometry: MultiPolygon<f64>,
    sw: (f64, f64),
    ne: (f64, f64),
}

impl RTreeObject for AdminBoundary {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let sw = [self.sw.0, self.sw.1];
        let ne = [self.ne.0, self.ne.1];
        AABB::from_corners(sw, ne)
    }
}

pub fn get_admin_hierarchies(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<AdminBoundary> {
    objs.values()
        .filter_map(|obj| {
            let relation = obj.relation()?;
            let name = relation.tags.get("name")?.clone();
            let admin_level = relation.tags.get("admin_level")?.parse().ok()?;
            let geometry = build_boundary(relation, objs)?;
            let rect = geometry.bounding_rect()?;
            let sw = rect.min().x_y();
            let ne = rect.max().x_y();
            let boundary = AdminBoundary {
                name,
                admin_level,
                geometry,
                sw,
                ne,
            };
            Some(boundary)
        })
        .collect()
}
