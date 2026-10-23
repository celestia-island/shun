//! The solar clock: where the sun stands over the machine's day — the
//! canonical `system`-mode light/dark rule every shell face shares with
//! hikari's webui theme clock (`useSolarTime`, ported arithmetic for
//! arithmetic).
//!
//! hikari's rule: the sun higher than +6° is day (light); civil
//! twilight and night are dark. First paint estimates the coordinates
//! from the timezone offset — the default latitude over 15° of
//! longitude per UTC hour, which puts solar noon at 12:00 local by
//! construction — and both faces re-evaluate every five minutes so
//! dawn and dusk flip a live face. The installer deliberately runs the
//! ESTIMATE only: it never phones a geolocation service (hikari's full
//! provider cascade stays a webui concern), and the timezone longitude
//! already tracks solar time to within DST-sized edges.

use std::time::{SystemTime, UNIX_EPOCH};

/// hikari's `DEFAULT_GEO_LOCATION` latitude (Shanghai) — the estimate's
/// latitude half. The longitude comes from the timezone offset, so the
/// pair lands on the local solar meridian wherever the machine runs.
const ESTIMATE_LAT: f64 = 31.23;

/// hikari's theme-clock cadence: how often the faces re-ask the sun
/// (dawn/dusk flips while a face sits open).
pub(crate) const SOLAR_TICK_SECS: u64 = 5 * 60;

const DEG: f64 = std::f64::consts::PI / 180.0;
const RAD: f64 = 180.0 / std::f64::consts::PI;

fn to_julian_date(unix_secs: f64) -> f64 {
    unix_secs / 86400.0 + 2440587.5
}

fn greenwich_sidereal_time(jd: f64) -> f64 {
    let t = (jd - 2451545.0) / 36525.0;
    let mut theta = (280.46061837 + 360.98564736629 * (jd - 2451545.0) + 0.000387933 * t * t
        - t * t * t / 38710000.0)
        % 360.0;
    if theta < 0.0 {
        theta += 360.0;
    }
    theta
}

/// The sun's declination and right ascension at a Julian date.
struct SunEquatorial {
    decl: f64,
    ra: f64,
}

fn sun_equatorial_coordinates(jd: f64) -> SunEquatorial {
    let t = (jd - 2451545.0) / 36525.0;
    let l0 = (280.46646 + 36000.76983 * t) % 360.0;
    let m = ((357.52911 + 35999.05029 * t) % 360.0) * DEG;
    let c = (1.9146 - 0.004817 * t) * m.sin() + (0.019993 - 0.000101 * t) * (2.0 * m).sin();
    let mut sun_lon = (l0 + c) % 360.0;
    if sun_lon < 0.0 {
        sun_lon += 360.0;
    }
    let omega = (125.04 - 1934.136 * t) * DEG;
    let lambda = sun_lon * DEG - 0.00569 * DEG - 0.00478 * DEG * omega.sin();
    let epsilon = (23.439291 - 0.013004 * t) * DEG;
    let decl = (epsilon.sin() * lambda.sin()).asin();
    let ra = (epsilon.cos() * lambda.sin()).atan2(lambda.cos());
    SunEquatorial { decl, ra }
}

/// The sun's altitude over `(lat, lng)` at the given Unix time, in
/// degrees — negative below the horizon.
pub(crate) fn solar_altitude(lat: f64, lng: f64, unix_secs: f64) -> f64 {
    let jd = to_julian_date(unix_secs);
    let sun = sun_equatorial_coordinates(jd);
    let mut ha_deg = greenwich_sidereal_time(jd) + lng - sun.ra * RAD;
    ha_deg = (((ha_deg + 180.0) % 360.0) + 360.0) % 360.0 - 180.0;
    let (lat, decl, ha) = (lat * DEG, sun.decl, ha_deg * DEG);
    (lat.sin() * decl.sin() + lat.cos() * decl.cos() * ha.cos()).asin() * RAD
}

/// hikari's day/night verdict at fixed coordinates: above +6° the face
/// runs light, civil twilight and night run dark.
pub(crate) fn prefers_dark_at(lat: f64, lng: f64, unix_secs: f64) -> bool {
    solar_altitude(lat, lng, unix_secs) <= 6.0
}

/// The first-paint coordinate estimate: hikari's default latitude over
/// the timezone offset's longitude.
pub(crate) fn estimate_coordinates() -> (f64, f64) {
    (ESTIMATE_LAT, f64::from(local_utc_offset_minutes()) / 4.0)
}

/// The `system`-mode verdict for right now, from the estimate.
pub(crate) fn timezone_estimate_dark() -> bool {
    let (lat, lng) = estimate_coordinates();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    prefers_dark_at(lat, lng, now)
}

/// The SDK's `TIME_ZONE_ID_DAYLIGHT` (windows-sys only exports
/// `TIME_ZONE_ID_INVALID`): the zone id naming the daylight bias active.
#[cfg(windows)]
const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

/// The machine's UTC offset in minutes (local − UTC), from the active
/// time-zone bias. `GetTimeZoneInformation` hands back the standard
/// bias plus a zone id naming which extra bias applies right now.
#[cfg(windows)]
fn local_utc_offset_minutes() -> i32 {
    use std::mem::zeroed;
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut info: TIME_ZONE_INFORMATION = unsafe { zeroed() };
    let zone = unsafe { GetTimeZoneInformation(&mut info) };
    // Bias is UTC = local + bias (minutes, west positive); daylight
    // flips the extra bias in.
    let extra = if zone == TIME_ZONE_ID_DAYLIGHT {
        info.DaylightBias
    } else {
        info.StandardBias
    };
    -(info.Bias + extra)
}

/// Non-Windows twin of [`local_utc_offset_minutes`]: the estimate floor
/// is UTC itself (longitude 0). The shell's GUI faces ship on Windows;
/// the other hosts only carry this module for the shared unit tests.
#[cfg(not(windows))]
fn local_utc_offset_minutes() -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed vectors recorded from hikari's `useSolarTime` (the same
    /// arithmetic, run over `lat = 31.23`, `lng = 120`) — the port must
    /// agree to the third decimal.
    fn cases() -> Vec<(&'static str, f64, f64, bool)> {
        vec![
            ("June noon", 1_782_014_400.0, 82.196, false),
            ("Jan midnight", 1_768_406_400.0, -79.782, true),
            ("Jan noon", 1_768_449_600.0, 37.595, false),
            ("Apr dusk", 1_775_817_900.0, -5.041, true),
            ("Apr dawn", 1_775_772_900.0, 6.936, false),
            ("June deep night", 1_781_982_000.0, -20.648, true),
            ("Oct dusk", 1_792_751_400.0, -15.608, true),
            ("Oct morning", 1_792_717_200.0, 31.992, false),
        ]
    }

    #[test]
    fn altitudes_match_the_hikari_reference_vectors() {
        for (label, unix, expected_alt, _) in cases() {
            let alt = solar_altitude(31.23, 120.0, unix);
            assert!(
                (alt - expected_alt).abs() < 0.001,
                "{label}: {alt} vs {expected_alt}"
            );
        }
    }

    #[test]
    fn the_verdict_follows_the_six_degree_band() {
        for (label, unix, _, expected_dark) in cases() {
            assert_eq!(
                prefers_dark_at(31.23, 120.0, unix),
                expected_dark,
                "{label}"
            );
        }
        // The band edge itself, around the April-dawn vector (whose
        // altitude climbs through +6° at ~0.21°/10 s): four minutes
        // earlier the sun still rides above the line (light), five
        // minutes earlier it has already crossed into civil twilight.
        assert!(!prefers_dark_at(31.23, 120.0, 1_775_772_660.0));
        assert!(prefers_dark_at(31.23, 120.0, 1_775_772_600.0));
    }

    #[test]
    fn solstice_noon_peaks_near_the_latitude_minus_declination() {
        // June solstice declination ≈ +23.44°: the noon altitude over
        // lat 31.23 bottoms out at 90 − |31.23 − 23.44| ≈ 82.2°.
        let alt = solar_altitude(31.23, 120.0, 1_782_014_400.0);
        assert!((82.2 - alt).abs() < 0.5, "{alt}");
    }

    #[cfg(windows)]
    #[test]
    fn the_offset_probe_lands_on_a_plausible_longitude() {
        let (_, lng) = estimate_coordinates();
        assert!((-180.0..=180.0).contains(&lng), "{lng}");
    }
}
