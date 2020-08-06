use super::geo::Length;
use super::geojson::{Entity, Geometry};
use super::items::{AdminBoundary, Object, Street};
use rand::random;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use std::collections::HashMap;
use std::error::Error;
use std::io::Write;

pub trait Output {
    fn write_geojson(&self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>>;
    fn write_json_lines(&self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>>;
}

#[derive(Serialize, Deserialize)]
struct JSONBBox {
    sw: [f64; 2],
    ne: [f64; 2],
}

#[derive(Serialize, Deserialize)]
struct JSONBoundary {
    name: String,
    admin_level: u8,
    bbox: JSONBBox,
}

impl Output for Vec<AdminBoundary> {
    fn write_json_lines(&self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        for boundary in self.iter() {
            let name = boundary.name.clone();
            let admin_level = boundary.admin_level;
            let (sw, ne) = boundary.geometry.sw_ne();
            let bbox = JSONBBox { sw, ne };
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

    fn write_geojson(&self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        let features = self
            .iter()
            .map(|boundary| {
                let coordinates = boundary.geometry.coordinates();
                let geometry = Geometry::MultiPolygon { coordinates };
                let properties = vec![
                    (String::from("name"), boundary.name.clone()),
                    (
                        String::from("admin_level"),
                        boundary.admin_level.to_string(),
                    ),
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
        writeln!(writer, "{}", string)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct JSONStreet {
    id: i64,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    boundary: Option<String>,
    length: f64,
    loc: (f64, f64),
}

impl Output for Vec<Object> {
    fn write_json_lines(&self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        for object in self.iter() {
            let json = match object {
                Object::Node(node) => to_string(node),
                Object::Way(way) => to_string(way),
                Object::Relation(rel) => to_string(rel),
            }?;
            writeln!(writer, "{}", json)?;
        }
        Ok(())
    }

    fn write_geojson(&self, _writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        unimplemented!();
    }
}

impl Output for Vec<Street> {
    fn write_json_lines(&self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        for street in self.iter() {
            let id = street.id();
            let loc = street.middle().ok_or("could not calculate middle")?;
            let name = street.name.clone();
            let boundary = street.boundary.clone();
            let length = street.length();
            let json_street = JSONStreet {
                id,
                name,
                boundary,
                length,
                loc,
            };
            let json = to_string(&json_street)?;
            writeln!(writer, "{}", json)?;
        }
        Ok(())
    }

    fn write_geojson(&self, writer: &mut dyn Write) -> Result<(), Box<dyn Error>> {
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
                let mut properties: HashMap<String, String> = HashMap::new();
                properties.insert("name".into(), street.name.clone());
                properties.insert("stroke".into(), random_color);
                if let Some(name) = &street.boundary {
                    properties.insert("boundary".into(), name.clone());
                }
                let entity = Entity::Feature {
                    geometry,
                    properties,
                };
                Some(entity)
            })
            .collect();

        let feature_collection = Entity::FeatureCollection { features };
        let string = to_string(&feature_collection)?;
        writeln!(writer, "{}", string)?;
        Ok(())
    }
}
