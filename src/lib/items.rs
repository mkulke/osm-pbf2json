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

pub enum Object {
    Node(osm::Node),
    Way(osm::Way),
    Relation(osm::Relation),
}

pub mod osm {
    use super::super::geo::{get_geo_info, Bounds, Location};
    use osmpbfreader::objects::Tags;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    pub struct Node {
        id: i64,
        #[serde(rename = "type")]
        osm_type: &'static str,
        lat: f64,
        lon: f64,
        tags: Tags,
    }

    impl Node {
        pub fn new(id: i64, lat: f64, lon: f64, tags: Tags) -> Self {
            Node {
                id,
                osm_type: "node",
                lat,
                lon,
                tags,
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
    }

    impl Way {
        pub fn new(id: i64, tags: Tags, coordinates: &[(f64, f64)]) -> Self {
            let (centroid, bounds) = get_geo_info(coordinates);
            Way {
                id,
                osm_type: "way",
                tags,
                centroid,
                bounds,
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    pub struct Relation {
        id: i64,
        #[serde(rename = "type")]
        osm_type: &'static str,
        tags: Tags,
        centroid: Option<Location>,
        bounds: Option<Bounds>,
    }

    impl Relation {
        pub fn new(id: i64, tags: Tags, coordinates: &[(f64, f64)]) -> Self {
            let (centroid, bounds) = get_geo_info(coordinates);
            Relation {
                id,
                osm_type: "relation",
                tags,
                centroid,
                bounds,
            }
        }
    }
}
