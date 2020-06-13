use geo::prelude::*;
use geo::COORD_PRECISION;
use geo_types::{Coordinate, Geometry, LineString, MultiPoint, Point, Polygon};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Serialize, Deserialize, Debug)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
}

impl PartialEq<Location> for Location {
    fn eq(&self, other: &Self) -> bool {
        let self_point = Point::new(self.lon, self.lat);
        let other_point = Point::new(other.lon, other.lat);
        let distance = self_point.haversine_distance(&other_point);
        // 5.0e-8 might be a good value
        distance < COORD_PRECISION.into()
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

impl PartialEq<Bounds> for Bounds {
    fn eq(&self, other: &Self) -> bool {
        let (self_ne, self_sw) = self.into();
        let (other_ne, other_sw) = other.into();
        self_ne == other_ne && self_sw == other_sw
    }
}

fn get_geometry(coordinates: Vec<(f64, f64)>) -> Option<Geometry<f64>> {
    let line_string: LineString<f64> = coordinates.into();
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
    fn get_middle(&self) -> Option<Location>;
}

fn get_closest_element<T: Into<Point<f64>> + Copy>(
    elements: impl IntoIterator<Item = T>,
    point: Point<f64>,
) -> Option<T> {
    elements.into_iter().min_by(|a, b| {
        let a_point: Point<f64> = (*a).into();
        let a_dis: f64 = point.euclidean_distance(&a_point);
        let b_point: Point<f64> = (*b).into();
        let b_dis: f64 = point.euclidean_distance(&b_point);
        a_dis.partial_cmp(&b_dis).unwrap()
    })
}

impl Centerable for Vec<(f64, f64)> {
    fn get_centroid(&self) -> Option<Location> {
        let geometry = get_geometry(self.clone())?;
        geometry.get_centroid()
    }

    fn get_middle(&self) -> Option<Location> {
        let line_string: LineString<f64> = self.clone().try_into().ok()?;
        let centroid = line_string.centroid()?;
        let closest_element = get_closest_element(line_string, centroid)?;
        Some(closest_element.into())
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

    fn get_middle(&self) -> Option<Location> {
        let multi_points: MultiPoint<f64> = self.clone().try_into().ok()?;
        let centroid = multi_points.centroid()?;
        let closest_element = get_closest_element(multi_points, centroid)?;
        Some(closest_element.into())
    }
}

pub fn get_geo_info(coordinates: Vec<(f64, f64)>) -> (Option<Location>, Option<Bounds>) {
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
        relative_eq!(a[0], b[0], epsilon = f64::EPSILON);
        relative_eq!(a[1], b[1], epsilon = f64::EPSILON);
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
    fn get_middle_for_line() {
        let coordinates = vec![(9., 50.), (9., 51.), (10., 51.)];
        // 1/m    2
        //
        //
        // 0
        approx_eq([9., 51.], coordinates.get_middle());
    }

    #[test]
    fn get_geo_info_open() {
        let coordinates = vec![(5., 49.), (6., 50.), (7., 49.)];
        let (centroid, bounds) = get_geo_info(coordinates);
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
        let (centroid, bounds) = get_geo_info(coordinates);
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
