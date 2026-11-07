//! Planet coordinates.
//!
//! World positions are f64 metres from the planet's centre in a planet-fixed
//! frame: +Y runs through the north pole, longitude 0 lies on +Z and
//! longitude grows towards +X. Nothing in world code may assume "+Y is up";
//! up depends on where you stand. For rendering and local physics, convert to
//! a [`LocalFrame`]: a flat tangent frame (f32-friendly near its origin) whose
//! axes follow Engine's convention.

use crate::det;
use glam::{DVec3, Vec3};

/// Mean radius of the home planet, in metres (Earth-like on purpose).
pub const HOME_RADIUS_M: f64 = 6_371_000.0;

/// A planet: a sphere with a radius. Terrain height is measured from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Planet {
    pub radius: f64,
}

impl Default for Planet {
    fn default() -> Self {
        Self {
            radius: HOME_RADIUS_M,
        }
    }
}

/// Latitude and longitude in radians. Latitude in `[-π/2, π/2]`, longitude in `(-π, π]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    pub fn from_degrees(lat: f64, lon: f64) -> Self {
        Self::new(lat.to_radians(), lon.to_radians())
    }

    /// The unit vector from the planet centre through this point.
    pub fn to_unit(self) -> DVec3 {
        let (sl, cl) = det::sin_cos(self.lat);
        let (so, co) = det::sin_cos(self.lon);
        DVec3::new(cl * so, sl, cl * co)
    }

    /// The direction of a vector from the centre; the zero vector maps to (0, 0).
    pub fn from_dir(d: DVec3) -> Self {
        let len = d.length();
        if len == 0.0 {
            return Self::new(0.0, 0.0);
        }
        Self::new(
            det::asin((d.y / len).clamp(-1.0, 1.0)),
            det::atan2(d.x, d.z),
        )
    }
}

/// A position in planet-fixed coordinates (metres from the centre).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PlanetPos(pub DVec3);

impl PlanetPos {
    /// The point at `height` metres above the planet's sphere.
    pub fn at(planet: &Planet, ll: LatLon, height: f64) -> Self {
        Self(ll.to_unit() * (planet.radius + height))
    }

    pub fn lat_lon(self) -> LatLon {
        LatLon::from_dir(self.0)
    }

    /// Height above the planet's sphere.
    pub fn height(self, planet: &Planet) -> f64 {
        self.0.length() - planet.radius
    }

    /// Unit up vector here (away from the centre).
    pub fn up(self) -> DVec3 {
        self.0.normalize_or(DVec3::Y)
    }

    /// Straight-line distance.
    pub fn distance(self, other: PlanetPos) -> f64 {
        self.0.distance(other.0)
    }

    /// Distance along the sphere's surface between the points' ground positions.
    pub fn surface_distance(self, other: PlanetPos, planet: &Planet) -> f64 {
        let (a, b) = (self.up(), other.up());
        // atan2 of cross and dot stays accurate for tiny and near-antipodal angles.
        let angle = det::atan2(a.cross(b).length(), a.dot(b));
        angle * planet.radius
    }
}

/// A flat frame tangent to the planet at `origin`, for rendering, local
/// physics and city layouts.
///
/// Local axes follow Engine and glTF: **x = east, y = up, z = south**
/// (so north is −z). Positions within a few kilometres stay precise in f32.
/// The ground curves away from the plane by about `d² / 2R` (0.3 m at 2 km),
/// which [`LocalFrame::ground_drop`] gives when it matters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalFrame {
    pub origin: PlanetPos,
    pub east: DVec3,
    pub up: DVec3,
    pub north: DVec3,
}

impl LocalFrame {
    /// The frame at `origin`. At the poles, where east is undefined, east is
    /// taken as +X.
    pub fn at(origin: PlanetPos) -> Self {
        let up = origin.up();
        let mut east = DVec3::Y.cross(up);
        if east.length_squared() < 1e-18 {
            east = DVec3::X;
        }
        let east = east.normalize();
        let north = up.cross(east);
        Self {
            origin,
            east,
            up,
            north,
        }
    }

    /// Planet position → local coordinates (x east, y up, z south).
    pub fn to_local(&self, p: PlanetPos) -> DVec3 {
        let d = p.0 - self.origin.0;
        DVec3::new(d.dot(self.east), d.dot(self.up), -d.dot(self.north))
    }

    /// Like [`Self::to_local`], narrowed to f32 for Engine.
    pub fn to_local_f32(&self, p: PlanetPos) -> Vec3 {
        self.to_local(p).as_vec3()
    }

    /// Local coordinates → planet position.
    pub fn to_planet(&self, local: DVec3) -> PlanetPos {
        PlanetPos(self.origin.0 + self.east * local.x + self.up * local.y - self.north * local.z)
    }

    /// A local direction → planet-fixed direction (no translation).
    pub fn dir_to_planet(&self, local: DVec3) -> DVec3 {
        self.east * local.x + self.up * local.y - self.north * local.z
    }

    /// How far the planet's surface lies below the tangent plane at a
    /// horizontal distance `d` from the origin.
    pub fn ground_drop(&self, planet: &Planet, d: f64) -> f64 {
        let r = planet.radius;
        r - (r * r - d * d).max(0.0).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    fn close(a: DVec3, b: DVec3, eps: f64) -> bool {
        a.distance(b) < eps
    }

    #[test]
    fn lat_lon_round_trip() {
        for (lat, lon) in [(0.0, 0.0), (45.0, 90.0), (-30.0, -120.0), (89.0, 179.0)] {
            let ll = LatLon::from_degrees(lat, lon);
            let back = LatLon::from_dir(ll.to_unit() * 1234.5);
            assert!((back.lat - ll.lat).abs() < 1e-12);
            assert!((back.lon - ll.lon).abs() < 1e-12);
        }
    }

    #[test]
    fn axes_match_the_convention() {
        assert!(close(
            LatLon::from_degrees(90.0, 0.0).to_unit(),
            DVec3::Y,
            1e-12
        ));
        assert!(close(
            LatLon::from_degrees(0.0, 0.0).to_unit(),
            DVec3::Z,
            1e-12
        ));
        assert!(close(
            LatLon::from_degrees(0.0, 90.0).to_unit(),
            DVec3::X,
            1e-12
        ));
    }

    #[test]
    fn height_and_position() {
        let planet = Planet::default();
        let p = PlanetPos::at(&planet, LatLon::from_degrees(10.0, 20.0), 150.0);
        assert!((p.height(&planet) - 150.0).abs() < 1e-6);
    }

    #[test]
    fn surface_distance_quarter_circle() {
        let planet = Planet::default();
        let a = PlanetPos::at(&planet, LatLon::from_degrees(0.0, 0.0), 0.0);
        let b = PlanetPos::at(&planet, LatLon::from_degrees(0.0, 90.0), 100.0);
        let expected = std::f64::consts::FRAC_PI_2 * planet.radius;
        assert!((a.surface_distance(b, &planet) - expected).abs() < 1e-3);
        assert_eq!(a.surface_distance(a, &planet), 0.0);
    }

    #[test]
    fn frame_is_orthonormal_and_right_handed() {
        let planet = Planet::default();
        for (lat, lon) in [
            (0.0, 0.0),
            (52.5, 13.4),
            (-60.0, 170.0),
            (90.0, 0.0),
            (-90.0, 0.0),
        ] {
            let f = LocalFrame::at(PlanetPos::at(&planet, LatLon::from_degrees(lat, lon), 0.0));
            for v in [f.east, f.up, f.north] {
                assert!((v.length() - 1.0).abs() < 1e-12);
            }
            assert!(f.east.dot(f.up).abs() < 1e-12);
            assert!(f.east.dot(f.north).abs() < 1e-12);
            // Local x, y, z = east, up, south must be right-handed.
            let south = -f.north;
            assert!(close(f.east.cross(f.up), south, 1e-12));
        }
    }

    #[test]
    fn north_points_to_the_pole() {
        let planet = Planet::default();
        let f = LocalFrame::at(PlanetPos::at(
            &planet,
            LatLon::from_degrees(40.0, 30.0),
            0.0,
        ));
        let step = f.to_planet(DVec3::new(0.0, 0.0, -1000.0)); // 1 km north
        assert!(step.lat_lon().lat > f.origin.lat_lon().lat);
        let step = f.to_planet(DVec3::new(1000.0, 0.0, 0.0)); // 1 km east
        assert!(step.lat_lon().lon > f.origin.lat_lon().lon);
    }

    #[test]
    fn local_round_trip_far_from_origin_of_planet() {
        let planet = Planet::default();
        let f = LocalFrame::at(PlanetPos::at(
            &planet,
            LatLon::from_degrees(33.3, -71.2),
            12.0,
        ));
        let local = DVec3::new(1234.5, 67.25, -2500.125);
        let back = f.to_local(f.to_planet(local));
        assert!(close(back, local, EPS));
        assert!(close(f.to_local(f.origin), DVec3::ZERO, EPS));
    }

    #[test]
    fn ground_drop_matches_curvature() {
        let planet = Planet::default();
        let f = LocalFrame::at(PlanetPos::at(&planet, LatLon::new(0.0, 0.0), 0.0));
        let drop = f.ground_drop(&planet, 2000.0);
        assert!((drop - 2000.0f64.powi(2) / (2.0 * planet.radius)).abs() < 1e-3);
    }
}
