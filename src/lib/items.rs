use super::geo::{BoundaryGeometry, SegmentGeometry};

pub struct AdminBoundary {
    pub name: String,
    pub admin_level: u8,
    pub geometry: BoundaryGeometry,
}

#[derive(Debug, Clone)]
pub struct Street {
    pub name: String,
    pub segments: Vec<Segment>,
    pub boundary: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub way_id: i64,
    pub geometry: SegmentGeometry,
}

pub mod osm {
    use super::super::geo::{get_geo_info, Bounds, Location};
    use osmpbfreader::objects::Tags;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum GeoInfo {
        Point {
            lon: f64,
            lat: f64,
        },
        Shape {
            centroid: Option<Location>,
            bounds: Option<Bounds>,
            #[serde(skip_serializing_if = "Option::is_none")]
            coordinates: Option<Vec<(f64, f64)>>,
        },
    }

    impl GeoInfo {
        pub fn new_shape(coordinates: &[(f64, f64)], retain_coordinates: bool) -> Self {
            let (centroid, bounds) = get_geo_info(coordinates);
            let coordinates = retain_coordinates.then(|| coordinates.into());
            GeoInfo::Shape {
                centroid,
                bounds,
                coordinates,
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub struct Object {
        id: i64,
        #[serde(rename = "type")]
        osm_type: &'static str,
        tags: Tags,
        #[serde(flatten)]
        geo_info: GeoInfo,
    }

    impl Object {
        pub fn new(id: i64, osm_type: &'static str, tags: Tags, geo_info: GeoInfo) -> Self {
            Self {
                id,
                osm_type,
                tags,
                geo_info,
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub struct Way {
        id: i64,
        #[serde(rename = "type")]
        osm_type: &'static str,
        tags: Tags,
        centroid: Option<Location>,
        bounds: Option<Bounds>,
        #[serde(skip_serializing_if = "Option::is_none")]
        coordinates: Option<Vec<(f64, f64)>>,
    }
}
