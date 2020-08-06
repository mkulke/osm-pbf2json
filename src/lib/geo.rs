use geo::prelude::*;
use geo::Closest;
use geo_types::{Coordinate, Geometry, Line, LineString, MultiPoint, MultiPolygon, Point, Polygon};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

const EQ_PRECISION: f64 = 1.0e-5;

#[derive(Serialize, Deserialize, Debug)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
}

impl From<(f64, f64)> for Location {
    fn from(tuple: (f64, f64)) -> Location {
        Location {
            lon: tuple.0,
            lat: tuple.1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SegmentGeometry {
    bounding_box: BoundingBox,
    line_string: LineString<f64>,
}

#[derive(Clone, Debug)]
pub struct BoundaryGeometry {
    bounding_box: BoundingBox,
    multi_polygon: MultiPolygon<f64>,
}

impl BoundaryGeometry {
    pub fn new(multi_polygon: MultiPolygon<f64>) -> Result<Self, &'static str> {
        let bounding_box = (&multi_polygon).try_into()?;
        Ok(BoundaryGeometry {
            bounding_box,
            multi_polygon,
        })
    }

    pub fn coordinates(&self) -> Vec<Vec<Vec<(f64, f64)>>> {
        self.multi_polygon
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
            .collect()
    }

    pub fn sw_ne(&self) -> ([f64; 2], [f64; 2]) {
        (self.bounding_box.sw, self.bounding_box.ne)
    }

    pub fn intersects(&self, geometry: &SegmentGeometry) -> bool {
        self.multi_polygon
            .0
            .iter()
            .any(|polygon| polygon.intersects(&geometry.line_string))
    }

    pub fn owns(&self, geometry: &SegmentGeometry) -> bool {
        if let Some(centroid) = geometry.line_string.centroid() {
            self.multi_polygon.contains(&centroid)
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
struct BoundingBox {
    sw: [f64; 2],
    ne: [f64; 2],
}

impl PartialEq<Location> for Location {
    fn eq(&self, other: &Self) -> bool {
        let self_point = Point::new(self.lon, self.lat);
        let other_point = Point::new(other.lon, other.lat);
        let distance = self_point.euclidean_distance(&other_point);
        distance < EQ_PRECISION
    }
}

impl PartialEq<Bounds> for Bounds {
    fn eq(&self, other: &Self) -> bool {
        let (self_ne, self_sw) = self.into();
        let (other_ne, other_sw) = other.into();
        self_ne == other_ne && self_sw == other_sw
    }
}

impl BoundingBox {
    pub fn pad(&self, distance: f64) -> BoundingBox {
        let sw: Point<f64> = self.sw.into();
        let ne: Point<f64> = self.ne.into();
        let padding: Point<f64> = (distance, distance).into();
        let sw_padded = sw - padding;
        let ne_padded = ne + padding;
        BoundingBox {
            sw: [sw_padded.lng(), sw_padded.lat()],
            ne: [ne_padded.lng(), ne_padded.lat()],
        }
    }
}

impl TryFrom<&MultiPolygon<f64>> for BoundingBox {
    type Error = &'static str;

    fn try_from(multi_polygon: &MultiPolygon<f64>) -> Result<Self, Self::Error> {
        let rect = multi_polygon
            .bounding_rect()
            .ok_or("cannot get bounding box for the given set of coordinates")?;
        let sw = [rect.min().x, rect.min().y];
        let ne = [rect.max().x, rect.max().y];
        Ok(BoundingBox { sw, ne })
    }
}

impl TryFrom<&LineString<f64>> for BoundingBox {
    type Error = &'static str;

    fn try_from(line_string: &LineString<f64>) -> Result<Self, Self::Error> {
        let rect = line_string
            .bounding_rect()
            .ok_or("cannot get bounding box for the given set of coordinates")?;
        let sw = [rect.min().x, rect.min().y];
        let ne = [rect.max().x, rect.max().y];
        Ok(BoundingBox { sw, ne })
    }
}

impl SegmentGeometry {
    pub fn new(coordinates: Vec<(f64, f64)>) -> Result<Self, &'static str> {
        let line_string: LineString<f64> = coordinates.into();
        let bounding_box: BoundingBox = (&line_string).try_into()?;
        let geometry = SegmentGeometry {
            bounding_box,
            line_string,
        };
        Ok(geometry)
    }

    pub fn len(&self) -> usize {
        self.line_string.points_iter().count()
    }

    pub fn sw_ne(&self) -> ([f64; 2], [f64; 2]) {
        (self.bounding_box.sw, self.bounding_box.ne)
    }

    pub fn padded_sw_ne(&self, distance: f64) -> ([f64; 2], [f64; 2]) {
        let BoundingBox { sw, ne } = self.bounding_box.pad(distance);
        (sw, ne)
    }
}

pub trait Length {
    fn length(&self) -> f64;
}

impl Length for SegmentGeometry {
    fn length(&self) -> f64 {
        let sw: Coordinate<f64> = self.bounding_box.sw.into();
        let ne: Coordinate<f64> = self.bounding_box.ne.into();
        let line = Line::new(sw, ne);
        line.euclidean_length()
    }
}

impl Length for Vec<&SegmentGeometry> {
    fn length(&self) -> f64 {
        self.iter().map(|segment| segment.length()).sum()
    }
}

impl From<&SegmentGeometry> for Vec<(f64, f64)> {
    fn from(geometry: &SegmentGeometry) -> Vec<(f64, f64)> {
        geometry
            .line_string
            .points_iter()
            .map(|c| (c.x(), c.y()))
            .collect()
    }
}

impl From<SegmentGeometry> for Vec<(f64, f64)> {
    fn from(geometry: SegmentGeometry) -> Vec<(f64, f64)> {
        geometry
            .line_string
            .points_iter()
            .map(|c| (c.x(), c.y()))
            .collect()
    }
}

impl From<&(f64, f64)> for Location {
    fn from(coordinates: &(f64, f64)) -> Self {
        Location {
            lon: coordinates.0,
            lat: coordinates.1,
        }
    }
}

impl From<Location> for [f64; 2] {
    fn from(loc: Location) -> Self {
        [loc.lon, loc.lat]
    }
}

impl From<Point<f64>> for Location {
    fn from(point: Point<f64>) -> Self {
        Location {
            lat: point.lat(),
            lon: point.lng(),
        }
    }
}

impl From<Coordinate<f64>> for Location {
    fn from(coordinate: Coordinate<f64>) -> Self {
        Location {
            lat: coordinate.y,
            lon: coordinate.x,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Bounds {
    e: f64,
    n: f64,
    s: f64,
    w: f64,
}

pub trait Midpoint {
    fn midpoint(&self) -> Option<(f64, f64)>;
}

impl Midpoint for Vec<&SegmentGeometry> {
    fn midpoint(&self) -> Option<(f64, f64)> {
        let flattened: Vec<_> = self
            .iter()
            .flat_map(|geometry| {
                let coordinates: Vec<(f64, f64)> = (*geometry).into();
                coordinates
            })
            .collect();
        let multi_points: MultiPoint<f64> = flattened.into();
        let centroid = multi_points.centroid()?;
        let closest = multi_points.closest_point(&centroid);
        match closest {
            Closest::Intersection(p) => Some((p.lng(), p.lat())),
            Closest::SinglePoint(p) => Some((p.lng(), p.lat())),
            _ => None,
        }
    }
}

impl From<&Bounds> for (Location, Location) {
    fn from(bounds: &Bounds) -> Self {
        let ne = Location {
            lon: bounds.n,
            lat: bounds.e,
        };
        let sw = Location {
            lon: bounds.s,
            lat: bounds.w,
        };

        (ne, sw)
    }
}

fn get_geometry(coordinates: &[(f64, f64)]) -> Option<Geometry<f64>> {
    let line_string: LineString<f64> = coordinates.to_vec().into();
    let first = line_string.points_iter().next()?;
    let last = line_string.points_iter().last()?;
    if first == last {
        let polygon = Polygon::new(line_string, vec![]);
        Some(Geometry::Polygon(polygon))
    } else {
        Some(Geometry::LineString(line_string))
    }
}

fn get_bounds(geometry: &Geometry<f64>) -> Option<Bounds> {
    let rect = match geometry {
        Geometry::LineString(ls) => ls.bounding_rect(),
        Geometry::Polygon(p) => p.bounding_rect(),
        _ => None,
    }?;
    Some(Bounds {
        e: rect.max().x,
        n: rect.max().y,
        s: rect.min().y,
        w: rect.min().x,
    })
}

pub trait Centerable {
    fn get_centroid(&self) -> Option<Location>;
}

impl Centerable for Vec<(f64, f64)> {
    fn get_centroid(&self) -> Option<Location> {
        let geometry = get_geometry(self)?;
        geometry.get_centroid()
    }
}

impl Centerable for Geometry<f64> {
    fn get_centroid(&self) -> Option<Location> {
        let point = match self {
            Geometry::LineString(ls) => ls.centroid(),
            Geometry::Polygon(p) => p.centroid(),
            _ => None,
        }?;
        Some(point.into())
    }
}

pub fn get_geo_info(coordinates: &[(f64, f64)]) -> (Option<Location>, Option<Bounds>) {
    if let Some(geo) = get_geometry(coordinates) {
        let centroid = geo.get_centroid();
        let bounds = get_bounds(&geo);
        return (centroid, bounds);
    }
    (None, None)
}

pub fn get_compound_coordinates(coordinates: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let multi_points: MultiPoint<_> = coordinates.into();
    let convex_hull = multi_points.convex_hull();
    convex_hull
        .exterior()
        .points_iter()
        .map(|p| (p.lng(), p.lat()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::*;

    fn approx_eq<T: Into<[f64; 2]>>(a: [f64; 2], o: Option<T>) {
        let b: [f64; 2] = o.unwrap().into();
        assert_relative_eq!(a[0], b[0], epsilon = f64::EPSILON);
        assert_relative_eq!(a[1], b[1], epsilon = f64::EPSILON);
    }

    #[test]
    fn get_centroid_for_line() {
        let coordinates = vec![(9., 50.), (9., 51.), (10., 51.)];
        // 1     2
        //  c
        //
        // 0
        approx_eq([9.25, 50.75], coordinates.get_centroid());
    }

    #[test]
    fn midpoint() {
        let coordinates = vec![(9., 50.), (9., 51.), (10., 51.)];
        let geometry_1 = SegmentGeometry::new(coordinates).unwrap();
        let coordinates = vec![(12., 51.), (12., 50.)];
        let geometry_2 = SegmentGeometry::new(coordinates).unwrap();
        // 1.1   1.2        2.0
        //
        //
        // 1.0              2.1
        let midpoint = vec![&geometry_1, &geometry_2]
            .midpoint()
            .map(|(lng, lat)| [lng, lat]);
        approx_eq([10., 51.], midpoint);
    }

    #[test]
    fn get_geo_info_open() {
        let coordinates = vec![(5., 49.), (6., 50.), (7., 49.)];
        let (centroid, bounds) = get_geo_info(&coordinates);
        let reference_loc = Location { lat: 49.5, lon: 6. };
        assert_eq!(centroid.unwrap(), reference_loc);
        let reference_bounds = Bounds {
            e: 7.,
            n: 50.,
            s: 49.,
            w: 5.,
        };
        assert_eq!(bounds.unwrap(), reference_bounds);
    }

    #[test]
    fn get_geo_info_closed() {
        let coordinates = vec![(5., 49.), (6., 50.), (7., 49.), (5., 49.)];
        let (centroid, bounds) = get_geo_info(&coordinates);
        let reference_loc = Location {
            lat: 49.333_333,
            lon: 6.,
        };
        assert_eq!(centroid.unwrap(), reference_loc);
        let reference_bounds = Bounds {
            e: 7.,
            n: 50.,
            s: 49.,
            w: 5.,
        };
        assert_eq!(bounds.unwrap(), reference_bounds);
    }
}
