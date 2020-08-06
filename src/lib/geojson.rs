use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Geometry {
    MultiLineString {
        coordinates: Vec<Vec<(f64, f64)>>,
    },
    MultiPolygon {
        coordinates: Vec<Vec<Vec<(f64, f64)>>>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Entity {
    Feature {
        properties: HashMap<String, String>,
        geometry: Geometry,
    },
    FeatureCollection {
        features: Vec<Entity>,
    },
}
