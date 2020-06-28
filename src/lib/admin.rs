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

impl AdminBoundary {
    fn geometry(&self) -> Geometry {
        let coordinates = self
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
        Geometry::MultiPolygon { coordinates }
    }
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
                let geometry = boundary.geometry();
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

pub fn get_boundaries(objs: &BTreeMap<OsmId, OsmObj>) -> Vec<AdminBoundary> {
    objs.values()
        .filter_map(|obj| {
            let relation = obj.relation()?;
            let boundary = relation.tags.get("boundary")?;
            if boundary != "administrative" {
                return None;
            }
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

#[cfg(test)]
mod get_boundaries {
    use super::*;
    use osm_boundaries_utils::osm_builder::{named_node, OsmBuilder};
    use osmpbfreader::objects::{OsmObj, Relation};

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

    #[test]
    fn geometry() {
        let mut builder = OsmBuilder::new();
        let rel_id = builder
            .relation()
            .outer(vec![
                named_node(3.4, 5.2, "start"),
                named_node(5.4, 5.1, "1"),
                named_node(2.4, 3.1, "2"),
                named_node(3.4, 5.2, "start"),
            ])
            .relation_id
            .into();

        let obj = builder.objects.get_mut(&rel_id).unwrap();
        let rel = obj.relation_mut().unwrap();
        rel.tags
            .insert("boundary".to_string(), "administrative".to_string());
        rel.tags.insert("name".to_string(), "some_name".to_string());
        rel.tags.insert("admin_level".to_string(), 11.to_string());

        let boundary = get_boundaries(&builder.objects).pop().unwrap();
        let geometry = boundary.geometry();
        match geometry {
            Geometry::MultiPolygon { coordinates } => {
                assert_eq!(coordinates.len(), 1);
                assert_eq!(coordinates[0].len(), 1);
                assert_eq!(coordinates[0][0].len(), 4);
            }
            _ => unreachable!(),
        }
        // assert_eq!(boundaries.len(), 1);
    }

    #[test]
    fn boundary_with_multiple_nodes() {
        let mut builder = OsmBuilder::new();
        let rel_id = builder
            .relation()
            .outer(vec![
                named_node(3.4, 5.2, "start"),
                named_node(5.4, 5.1, "1"),
                named_node(2.4, 3.1, "2"),
                named_node(3.4, 5.2, "start"),
            ])
            .relation_id
            .into();

        let obj = builder.objects.get_mut(&rel_id).unwrap();
        let rel = obj.relation_mut().unwrap();
        rel.tags
            .insert("boundary".to_string(), "administrative".to_string());
        rel.tags.insert("name".to_string(), "some_name".to_string());
        rel.tags.insert("admin_level".to_string(), 11.to_string());

        let boundaries = get_boundaries(&builder.objects);
        assert_eq!(boundaries.len(), 1);
    }

    #[test]
    fn relation_with_missing_tags() {
        let mut builder = OsmBuilder::new();
        let rel_id = builder
            .relation()
            .outer(vec![
                named_node(3.4, 5.2, "start"),
                named_node(5.4, 5.1, "1"),
                named_node(2.4, 3.1, "2"),
                named_node(3.4, 5.2, "start"),
            ])
            .relation_id
            .into();

        let obj = builder.objects.get_mut(&rel_id).unwrap();
        let rel = obj.relation_mut().unwrap();
        rel.tags.insert("boundary".to_string(), "wrong".to_string());
        rel.tags.insert("name".to_string(), "some_name".to_string());
        rel.tags.insert("admin_level".to_string(), 11.to_string());

        let boundaries = get_boundaries(&builder.objects);
        assert_eq!(boundaries.len(), 0);
    }
}
