use geo::prelude::*;
use geo::COORD_PRECISION;
use geo_types::{Geometry, LineString, MultiPoint, Point, Polygon};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Location {
    lat: f64,
    lon: f64,
}

impl PartialEq<Location> for Location {
    fn eq(&self, other: &Self) -> bool {
        let self_point = Point::new(self.lon, self.lat);
        let other_point = Point::new(other.lon, other.lat);
        let distance = self_point.haversine_distance(&other_point);
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

impl Location {
    pub fn is_close_to(&self, other: &Self) -> bool {
        let self_point = Point::new(self.lon, self.lat);
        let other_point = Point::new(other.lon, other.lat);
        let distance = self_point.euclidean_distance(&other_point);
        distance < 5.0e-8
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

fn get_centroid(geometry: &Geometry<f64>) -> Option<Location> {
    let point = match geometry {
        Geometry::LineString(ls) => ls.centroid(),
        Geometry::Polygon(p) => p.centroid(),
        _ => None,
    }?;
    Some(Location {
        lat: point.lat(),
        lon: point.lng(),
    })
}

pub fn get_geo_info(coordinates: Vec<(f64, f64)>) -> (Option<Location>, Option<Bounds>) {
    if let Some(geo) = get_geometry(coordinates) {
        let centroid = get_centroid(&geo);
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
