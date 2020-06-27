use super::geojson::{Entity, Geometry};
use geo::algorithm::bounding_rect::BoundingRect;
use geo_types::MultiPolygon;
use osm_boundaries_utils::build_boundary;
use osmpbfreader::objects::{OsmId, OsmObj};
use rstar::{RTreeObject, AABB};
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;

pub trait AdminOutput {
    fn to_geojson(&self) -> Result<String, Box<dyn Error>>;
    fn write_json_lines(self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>>;
}

pub struct AdminBoundary {
    name: String,
    admin_level: u8,
    geometry: MultiPolygon<f64>,
    sw: (f64, f64),
    ne: (f64, f64),
}

#[derive(Serialize, Deserialize)]
struct JSONBBox {
    sw: (f64, f64),
    ne: (f64, f64),
}

#[derive(Serialize, Deserialize)]
struct JSONBoundary {
    name: String,
    admin_level: u8,
    bbox: JSONBBox,
}

impl AdminOutput for Vec<AdminBoundary> {
    fn write_json_lines(self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        for boundary in self.iter() {
            let name = boundary.name.clone();
            let admin_level = boundary.admin_level;
            let bbox = JSONBBox {
                sw: boundary.sw,
                ne: boundary.ne,
            };
            let json_boundary = JSONBoundary {
                name,
                admin_level,
                bbox,
            };
            let json = to_string(&json_boundary)?;
            writeln!(writer, "{}", json)?;
        }
        Ok(())
    }

    fn to_geojson(&self) -> Result<String, Box<dyn Error>> {
        let features = self
            .iter()
            .map(|boundary| {
                let coordinates = boundary
                    .geometry
                    .clone()
                    .into_iter()
                    .map(|polygon| {
                        let (exterior, interiours) = polygon.into_inner();
                        let mut rings = vec![exterior];
                        rings.extend(interiours);
                        rings
                    })
                    .map(|line_strings| {
                        line_strings
                            .iter()
                            .map(|ls| ls.points_iter().map(|p| (p.x(), p.y())).collect())
                            .collect()
                    })
                    .collect();
                let geometry = Geometry::MultiPolygon { coordinates };
                let properties = vec![
                    ("name".to_string(), boundary.name.clone()),
                    ("admin_level".to_string(), boundary.admin_level.to_string()),
                ]
                .into_iter()
                .collect();
                Entity::Feature {
                    geometry,
                    properties,
                }
            })
            .collect();
        let feature_collection = Entity::FeatureCollection { features };
        let string = to_string(&feature_collection)?;
        Ok(string)
    }
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
