use super::geo::BoundaryGeometry;
use super::geo::SegmentGeometry;
use osmpbfreader::objects::WayId;

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
    pub way_id: WayId,
    pub geometry: SegmentGeometry,
}
