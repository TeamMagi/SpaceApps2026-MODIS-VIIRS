use crate::config::{MatchingConfig, PipelineConfig, VzaConfig};
use crate::dedup::{haversine_km, remove_interscan_duplicates_indexed_with};
use crate::fleet::{self, FleetObservation};
use crate::h3index::{TimedH3Index, k_ring_for_radius, shift_time_clamped};
use crate::models::*;
use crate::sama;
use crate::static_mask::StaticMask;
use crate::uncertainty::{self, SensorFamily, UncertaintyParams};
use crate::vza;

pub(crate) const MATCH_RADIUS_KM: f64 = 2.0;
// 180 minutes: orbital geometry separates MODIS and VIIRS overpasses of the same fire
// (min 34 min, median ~158 min in four live NRT regions), so a narrow window yields few matches.
pub(crate) const MATCH_WINDOW_SECS: i64 = 10_800;
// Radiometric comparison pairs need near-coincident overpasses; the literature accepts 5 min
// and at most 20 min.
pub(crate) const SIMULTANEOUS_WINDOW_SECS: i64 = 1_200;
// Sub-pixels of one VIIRS overpass over a footprint are seconds apart, while consecutive
// platforms are at least 43 min apart, so 10 min separates passes without splitting a swath.
pub(crate) const VIIRS_PASS_WINDOW_SECS: i64 = 600;
// A 1 km nadir pixel's half-diagonal is ~0.71 km; allow for VIIRS geolocation tolerance.
const MIN_FOOTPRINT_RADIUS_KM: f64 = 0.75;

/// Output of [`harmonize_with_links`]: the harmonized hotspots plus, for every input row,
/// which output it became.
#[derive(Debug, Clone)]
pub struct HarmonizeLinks {
    pub hotspots: Vec<HarmonizedHotspot>,
    /// `modis_links[i]` is the index in `hotspots` produced by input MODIS row `i`,
    /// or `None` if the SAMA filter, the static mask or interscan dedup removed that row.
    pub modis_links: Vec<Option<usize>>,
    /// Same as `modis_links`, for the VIIRS input rows.
    pub viirs_links: Vec<Option<usize>>,
    /// Input rows dropped as persistent static sources (see [`crate::static_mask`]).
    pub static_masked: usize,
    /// Nighttime VIIRS input rows dropped inside the South Atlantic Magnetic Anomaly.
    pub sama_removed: usize,
    pub interscan_removed: usize,
    pub fleet_merged: usize,
    /// Number of `Matched` outputs (one per MODIS observation that found VIIRS pixels).
    pub matched_pairs: usize,
    /// VIIRS observations absorbed into a `Matched` output beyond the first one of each match.
    pub viirs_aggregated: usize,
    /// Indices in `hotspots` of `Matched` outputs whose MODIS and VIIRS times differ by at most
    /// `matching.simultaneous_window_secs`; only these are suited to radiometric estimation.
    pub simultaneous_matched: Vec<usize>,
}

/// Full harmonization pipeline:
/// 0. Drop SAMA nighttime VIIRS noise and persistent static-source cells (each optional)
/// 1. Deduplicate interscan overlaps
/// 2. Consolidate multi-platform MODIS and VIIRS fleet detections
/// 3. Cross-sensor spatial-temporal matching: every VIIRS observation goes to its nearest MODIS
///    observation, and all VIIRS pixels of the seed VIIRS pass are summed into one record
/// 4. Quality-priority fusion (VIIRS preferred) and non-mutating MODIS VZA correction
///
/// Cross-sensor semantics: `HotspotSource::Matched` represents observations of the same
/// fire event by both sensors rather than simultaneous observations. In four live FIRMS NRT
/// regions, co-located MODIS/VIIRS pairs were at least 34 minutes apart (median ~158 minutes).
/// In the 2000–2025 country archive, near-simultaneous Aqua/S-NPP pairs are plentiful and form
/// `simultaneous_matched` for measuring `k_c`; Terra does not coincide with VIIRS, and S-NPP /
/// NOAA-20 are ~50 minutes apart, so the 180-minute window remains useful for same-fire matching.
/// The VZA curve and VIIRS sigma 0.08 remain literature-based/provisional, not re-fitted here;
/// archive pairs allow a diagnostic check, which this code does not perform.
pub fn harmonize(modis: Vec<Hotspot>, viirs: Vec<Hotspot>) -> Vec<HarmonizedHotspot> {
    harmonize_with_config(modis, viirs, &PipelineConfig::default())
}

/// Same as [`harmonize`] with configurable thresholds.
pub fn harmonize_with_config(
    modis: Vec<Hotspot>,
    viirs: Vec<Hotspot>,
    config: &PipelineConfig,
) -> Vec<HarmonizedHotspot> {
    harmonize_with_links_config(modis, viirs, config).hotspots
}

/// Runs the same pipeline as [`harmonize`] and records which output each input row became.
pub fn harmonize_with_links(modis: Vec<Hotspot>, viirs: Vec<Hotspot>) -> HarmonizeLinks {
    harmonize_with_links_config(modis, viirs, &PipelineConfig::default())
}

/// Same as [`harmonize_with_links`] with configurable thresholds.
pub fn harmonize_with_links_config(
    modis: Vec<Hotspot>,
    viirs: Vec<Hotspot>,
    config: &PipelineConfig,
) -> HarmonizeLinks {
    harmonize_with_links_masked(modis, viirs, config, None)
}

/// Same as [`harmonize_with_links_config`]; `mask` replaces the static mask the pipeline would
/// otherwise build from this batch, so sub-batches (e.g. one month of a year) can share the
/// verdict of the whole year. It is ignored when `config.static_mask.enabled` is false.
pub fn harmonize_with_links_masked(
    mut modis: Vec<Hotspot>,
    mut viirs: Vec<Hotspot>,
    config: &PipelineConfig,
    mask: Option<&StaticMask>,
) -> HarmonizeLinks {
    let modis_input_len = modis.len();
    let viirs_input_len = viirs.len();

    // Step 0: SAMA first, so its random noise does not count towards cell persistence.
    let mut modis_pre: Vec<usize> = (0..modis_input_len).collect();
    let mut viirs_pre: Vec<usize> = (0..viirs_input_len).collect();
    let mut sama_removed = 0;
    if config.sama.enabled {
        sama_removed += retain_tracked(&mut modis, &mut modis_pre, |h| sama::is_sama_noise(h, &config.sama));
        sama_removed += retain_tracked(&mut viirs, &mut viirs_pre, |h| sama::is_sama_noise(h, &config.sama));
    }
    let mut static_masked = 0;
    if config.static_mask.enabled {
        let built;
        let mask = match mask {
            Some(mask) => mask,
            None => {
                built = StaticMask::from_detections(modis.iter().chain(&viirs), &config.static_mask);
                &built
            }
        };
        if !mask.is_empty() {
            static_masked += retain_tracked(&mut modis, &mut modis_pre, |h| mask.contains(h));
            static_masked += retain_tracked(&mut viirs, &mut viirs_pre, |h| mask.contains(h));
        }
    }
    let modis_filtered_len = modis.len();
    let viirs_filtered_len = viirs.len();

    // Step 1: Remove interscan duplicates (also sorts both vectors by `acquired` ascending)
    let modis_origin = remove_interscan_duplicates_indexed_with(&mut modis, &config.dedup);
    let viirs_origin = remove_interscan_duplicates_indexed_with(&mut viirs, &config.dedup);
    let interscan_removed =
        (modis_filtered_len - modis.len()) + (viirs_filtered_len - viirs.len());

    // Step 2: Consolidate both fleets before cross-sensor matching so sibling detections
    // (Terra/Aqua and SNPP/NOAA-20/NOAA-21) do not leak into the output as duplicate singletons.
    let (modis_fleet, modis_groups) = fleet::consolidate_modis_fleet_indexed_with(&modis, &config.fleet, &config.vza);
    let (viirs_fleet, viirs_groups) = fleet::consolidate_viirs_fleet_indexed_with(&viirs, &config.fleet, &config.vza);
    let fleet_merged =
        (modis.len() - modis_fleet.len()) + (viirs.len() - viirs_fleet.len());

    // Step 3 & 4: Match and fuse using time-sliced H3 spatial index candidate lookup
    let use_spatial = config.matching.radius_km.is_finite();
    let index = use_spatial.then(|| {
        TimedH3Index::build(
            viirs_fleet
                .iter()
                .map(|v| (v.hotspot.lat, v.hotspot.lon, v.hotspot.acquired)),
        )
    });
    let mut result = Vec::with_capacity(modis_fleet.len() + viirs_fleet.len());
    let mut modis_fleet_out = vec![0usize; modis_fleet.len()];
    let mut viirs_fleet_out: Vec<Option<usize>> = vec![None; viirs_fleet.len()];
    let mut matched_pairs = 0usize;
    let mut viirs_aggregated = 0usize;
    let mut simultaneous_result_idx = Vec::new();
    let search_k = if use_spatial {
        k_ring_for_radius(config.matching.radius_km)
    } else {
        0
    };
    let pad = config.matching.window_secs.saturating_add(1);
    let mut candidates = Vec::new();

    // Each VIIRS observation belongs to its nearest eligible MODIS observation only, so a pixel
    // between two separate fires is not swallowed by the one processed first.
    let mut owner: Vec<Option<(f64, usize)>> = vec![None; viirs_fleet.len()];
    for (mi, m) in modis_fleet.iter().enumerate() {
        let mh = &m.hotspot;
        let t_min = shift_time_clamped(mh.acquired, pad.saturating_neg());
        let t_max = shift_time_clamped(mh.acquired, pad);
        let used_index = index.as_ref().is_some_and(|idx| {
            idx.candidates_within_into(mh.lat, mh.lon, search_k, t_min, t_max, &mut candidates)
        });

        let mut consider = |j: usize| {
            if let Some(dist) = eligible_distance(mh, &viirs_fleet[j].hotspot, &config.matching)
                && owner[j].is_none_or(|(best, _)| dist < best)
            {
                owner[j] = Some((dist, mi));
            }
        };

        if used_index {
            for &j in &candidates {
                consider(j);
            }
        } else {
            for j in 0..viirs_fleet.len() {
                consider(j);
            }
        }
    }
    let mut owned: Vec<Vec<usize>> = vec![Vec::new(); modis_fleet.len()];
    for (j, o) in owner.iter().enumerate() {
        if let Some((_, mi)) = o {
            owned[*mi].push(j);
        }
    }

    for (mi, m) in modis_fleet.iter().enumerate() {
        modis_fleet_out[mi] = result.len();
        let Some(pass) = viirs_pass_members(m, &owned[mi], &viirs_fleet, &config.matching) else {
            result.push(from_modis_only_with(m, &config.vza, &config.uncertainty));
            continue;
        };

        let seed = &viirs_fleet[pass[0]].hotspot;
        let dt = (seed.acquired - m.hotspot.acquired).num_seconds().abs();
        if dt <= config.matching.simultaneous_window_secs {
            simultaneous_result_idx.push(result.len());
        }
        for &j in &pass {
            viirs_fleet_out[j] = Some(result.len());
        }
        matched_pairs += 1;
        viirs_aggregated += pass.len() - 1;
        let members: Vec<&FleetObservation> = pass.iter().map(|&j| &viirs_fleet[j]).collect();
        result.push(fuse_matched(m, &members, &config.uncertainty));
    }

    // Unmatched VIIRS detections
    for (j, v) in viirs_fleet.iter().enumerate() {
        if viirs_fleet_out[j].is_none() {
            viirs_fleet_out[j] = Some(result.len());
            result.push(from_viirs_only(v, &config.uncertainty));
        }
    }

    // Stable index sort preserves the exact ordering of `result.sort_by_key(|a| a.acquired)`
    // while mapping fleet observations to their final positions in `hotspots`.
    let mut order: Vec<usize> = (0..result.len()).collect();
    order.sort_by_key(|&k| result[k].acquired);
    let mut final_pos = vec![0usize; result.len()];
    for (new_pos, &old_pos) in order.iter().enumerate() {
        final_pos[old_pos] = new_pos;
    }
    let mut slots: Vec<Option<HarmonizedHotspot>> = result.into_iter().map(Some).collect();
    let hotspots: Vec<HarmonizedHotspot> = order
        .iter()
        .map(|&k| slots[k].take().expect("each output taken once"))
        .collect();

    let mut modis_links = vec![None; modis_input_len];
    for (mi, members) in modis_groups.iter().enumerate() {
        let out = final_pos[modis_fleet_out[mi]];
        for &dedup_idx in members {
            modis_links[modis_pre[modis_origin[dedup_idx]]] = Some(out);
        }
    }

    let mut viirs_links = vec![None; viirs_input_len];
    for (vj, members) in viirs_groups.iter().enumerate() {
        let out = final_pos[viirs_fleet_out[vj].expect("every VIIRS fleet group produces or joins an output")];
        for &dedup_idx in members {
            viirs_links[viirs_pre[viirs_origin[dedup_idx]]] = Some(out);
        }
    }

    let mut simultaneous_matched: Vec<usize> =
        simultaneous_result_idx.iter().map(|&k| final_pos[k]).collect();
    simultaneous_matched.sort_unstable();

    HarmonizeLinks {
        hotspots,
        modis_links,
        viirs_links,
        static_masked,
        sama_removed,
        interscan_removed,
        fleet_merged,
        matched_pairs,
        viirs_aggregated,
        simultaneous_matched,
    }
}

/// Distance in km when `v` is inside the matching window and radius of `m`.
fn eligible_distance(m: &Hotspot, v: &Hotspot, config: &MatchingConfig) -> Option<f64> {
    let dt = (v.acquired - m.acquired).num_seconds().abs();
    if dt > config.window_secs {
        return None;
    }
    let dist = haversine_km(m.lat, m.lon, v.lat, v.lon);
    (dist <= config.radius_km).then_some(dist)
}

fn footprint_radius_km(m: &Hotspot, config: &MatchingConfig) -> f64 {
    if m.scan.is_finite() && m.scan > 0.0 && m.track.is_finite() && m.track > 0.0 {
        (0.5 * m.scan.hypot(m.track))
            .max(MIN_FOOTPRINT_RADIUS_KM)
            .min(config.radius_km)
    } else {
        config.radius_km
    }
}

/// Picks the VIIRS pass of a MODIS observation from the observations it owns: the best-scoring
/// one seeds the pass anywhere within the matching radius. Other owned observations join within
/// `VIIRS_PASS_WINDOW_SECS` of the seed and the MODIS-centred scan/track half-diagonal (at least
/// 0.75 km, capped at the matching radius); missing or invalid dimensions use the matching radius.
/// Returns indices with the seed first, the rest ascending; `None` if nothing is owned.
fn viirs_pass_members(
    m: &FleetObservation,
    owned: &[usize],
    viirs_fleet: &[FleetObservation],
    config: &MatchingConfig,
) -> Option<Vec<usize>> {
    let mh = &m.hotspot;
    let mut seed: Option<(usize, f64)> = None;
    for &j in owned {
        let vh = &viirs_fleet[j].hotspot;
        let dt = (vh.acquired - mh.acquired).num_seconds().abs();
        let dist = haversine_km(mh.lat, mh.lon, vh.lat, vh.lon);
        let score = match_score_with(dist, dt, config);
        if seed.is_none_or(|(_, best)| score > best) {
            seed = Some((j, score));
        }
    }
    let (seed, _) = seed?;

    let seed_time = viirs_fleet[seed].hotspot.acquired;
    let radius_km = footprint_radius_km(mh, config);
    let mut pass = vec![seed];
    pass.extend(owned.iter().copied().filter(|&j| {
        j != seed
            && (viirs_fleet[j].hotspot.acquired - seed_time)
                .num_seconds()
                .abs()
                <= VIIRS_PASS_WINDOW_SECS
            && haversine_km(
                mh.lat,
                mh.lon,
                viirs_fleet[j].hotspot.lat,
                viirs_fleet[j].hotspot.lon,
            ) <= radius_km
    }));
    Some(pass)
}

/// Sums the FRP of VIIRS pixels of one pass into a single observation located at the seed.
/// `n_obs` adds up for reporting, while [`fuse_matched`] combines member uncertainties in
/// quadrature because unequal pixel FRPs do not shrink as `1/sqrt(n)`.
fn sum_viirs_pass(members: &[&FleetObservation]) -> FleetObservation {
    let mut hotspot = members[0].hotspot.clone();
    hotspot.frp = members.iter().map(|v| v.hotspot.frp).sum();
    hotspot.confidence = members
        .iter()
        .map(|v| v.hotspot.confidence)
        .max()
        .unwrap_or(hotspot.confidence);

    let mut platforms: Vec<Sensor> = members.iter().flat_map(|v| v.platforms.iter().copied()).collect();
    platforms.sort();
    platforms.dedup();

    FleetObservation {
        hotspot,
        frp_mean: members.iter().map(|v| v.frp_mean).sum(),
        frp_corrected_mean: members.iter().map(|v| v.frp_corrected_mean).sum(),
        n_obs: members
            .iter()
            .fold(0u16, |acc, v| acc.saturating_add(v.n_obs)),
        platforms,
    }
}

/// Drops the rows for which `drop` holds, keeping `origin` aligned with `rows`; returns the count.
fn retain_tracked(
    rows: &mut Vec<Hotspot>,
    origin: &mut Vec<usize>,
    drop: impl Fn(&Hotspot) -> bool,
) -> usize {
    let keep: Vec<bool> = rows.iter().map(|h| !drop(h)).collect();
    let before = rows.len();
    let mut flags = keep.iter();
    rows.retain(|_| *flags.next().expect("one flag per row"));
    let mut flags = keep.iter();
    origin.retain(|_| *flags.next().expect("one flag per origin"));
    before - rows.len()
}

fn fuse_matched(
    m: &FleetObservation,
    viirs_members: &[&FleetObservation],
    uncertainty_params: &UncertaintyParams,
) -> HarmonizedHotspot {
    let v = sum_viirs_pass(viirs_members);
    let mh = &m.hotspot;
    let vh = &v.hotspot;
    let mut platforms = Vec::with_capacity(m.platforms.len() + v.platforms.len());
    platforms.extend_from_slice(&m.platforms);
    platforms.extend_from_slice(&v.platforms);
    platforms.sort();
    platforms.dedup();

    let n_obs = m.n_obs.saturating_add(v.n_obs);
    // The FRP value comes only from the VIIRS members, so the confidence interval must combine
    // only the VIIRS pass members; including MODIS members would narrow an interval that no
    // MODIS observation informs. `n_obs` still reports the total member count.
    let parts: Vec<(f64, u16)> = viirs_members
        .iter()
        .map(|member| (member.frp_corrected_mean, member.n_obs))
        .collect();
    let est = uncertainty::frp_uncertainty_for_sum(
        &parts,
        SensorFamily::from(HotspotSource::Matched),
        uncertainty_params,
    );

    // Quality-priority: prefer VIIRS FRP (Zheng et al. 2026)
    HarmonizedHotspot {
        lat: vh.lat,
        lon: vh.lon,
        frp: vh.frp,
        frp_corrected: v.frp_corrected_mean,
        frp_lower: est.lower,
        frp_upper: est.upper,
        vza_factor: None,
        n_obs,
        platforms,
        confidence: higher_confidence(mh.confidence, vh.confidence),
        source: HotspotSource::Matched,
        acquired: vh.acquired,
        daynight: vh.daynight,
    }
}

fn from_modis_only_with(
    m: &FleetObservation,
    vza_config: &VzaConfig,
    uncertainty_params: &UncertaintyParams,
) -> HarmonizedHotspot {
    let mh = &m.hotspot;
    let factor = vza::vza_factor_with(mh, vza_config);
    // Per-detection latitude scaling is not applied: completeness differences between
    // MODIS and VIIRS belong to cell-level total FRP in the pre-VIIRS era, not to
    // individual detections (which would inflate 2000-2011 per-fire FRP and double-count
    // small fires already captured as ViirsOnly once VIIRS is present).
    let frp_corrected = m.frp_corrected_mean;

    // Random sensor/VZA errors shrink by 1/√n_obs across fleet members, whereas
    // MODIS scale/proxy uncertainty (`modis_scale_sigma`) is shared and stays unscaled.
    let est = uncertainty::frp_uncertainty_for_obs(
        frp_corrected,
        factor,
        SensorFamily::from(HotspotSource::ModisOnly),
        None,
        m.n_obs,
        uncertainty_params,
    );

    HarmonizedHotspot {
        lat: mh.lat,
        lon: mh.lon,
        frp: mh.frp,
        frp_corrected,
        frp_lower: est.lower,
        frp_upper: est.upper,
        vza_factor: factor,
        n_obs: m.n_obs,
        platforms: m.platforms.clone(),
        confidence: mh.confidence,
        source: HotspotSource::ModisOnly,
        acquired: mh.acquired,
        daynight: mh.daynight,
    }
}

#[cfg(test)]
fn from_modis_only(m: &FleetObservation) -> HarmonizedHotspot {
    from_modis_only_with(m, &VzaConfig::default(), &UncertaintyParams::default())
}

fn from_viirs_only(
    v: &FleetObservation,
    uncertainty_params: &UncertaintyParams,
) -> HarmonizedHotspot {
    let vh = &v.hotspot;
    // Random sensor noise shrinks by 1/√n_obs across merged VIIRS passes; no VZA or scale term.
    let est = uncertainty::frp_uncertainty_for_obs(
        v.frp_corrected_mean,
        None,
        SensorFamily::from(HotspotSource::ViirsOnly),
        None,
        v.n_obs,
        uncertainty_params,
    );

    HarmonizedHotspot {
        lat: vh.lat,
        lon: vh.lon,
        frp: vh.frp,
        frp_corrected: v.frp_corrected_mean,
        frp_lower: est.lower,
        frp_upper: est.upper,
        vza_factor: None,
        n_obs: v.n_obs,
        platforms: v.platforms.clone(),
        confidence: vh.confidence,
        source: HotspotSource::ViirsOnly,
        acquired: vh.acquired,
        daynight: vh.daynight,
    }
}

fn match_score_with(dist_km: f64, dt_secs: i64, config: &MatchingConfig) -> f64 {
    let spatial = 1.0 - (dist_km / config.radius_km);
    // A zero window only admits dt == 0, which is a perfect temporal match (and avoids 0/0).
    let temporal = if config.window_secs == 0 {
        1.0
    } else {
        1.0 - (dt_secs as f64 / config.window_secs as f64)
    };
    0.6 * spatial + 0.4 * temporal
}

fn higher_confidence(a: Confidence, b: Confidence) -> Confidence {
    use Confidence::*;
    match (a, b) {
        (High, _) | (_, High) => High,
        (Nominal, _) | (_, Nominal) => Nominal,
        _ => Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dedup::remove_interscan_duplicates;
    use chrono::{Duration, TimeZone, Utc};

    fn make_hotspot(
        sensor: Sensor,
        lat: f64,
        lon: f64,
        offset_secs: i64,
        frp: f64,
        confidence: Confidence,
    ) -> Hotspot {
        let base = Utc.with_ymd_and_hms(2025, 7, 15, 12, 0, 0).unwrap();
        Hotspot {
            lat,
            lon,
            brightness: 330.0,
            frp,
            confidence,
            sensor,
            acquired: base + Duration::seconds(offset_secs),
            scan: 0.0,
            track: 0.0,
            satellite: String::new(),
            daynight: DayNight::Day,
            vza: None,
            fire_type: FireType::VegetationFire,
        }
    }

    /// Nearest-MODIS ownership by plain scan over all pairs, independent of the H3 index.
    fn brute_force_owned(
        modis: &[FleetObservation],
        viirs: &[FleetObservation],
        config: &MatchingConfig,
    ) -> Vec<Vec<usize>> {
        let mut owned = vec![Vec::new(); modis.len()];
        for (j, v) in viirs.iter().enumerate() {
            let mut best: Option<(f64, usize)> = None;
            for (mi, m) in modis.iter().enumerate() {
                if let Some(d) = eligible_distance(&m.hotspot, &v.hotspot, config)
                    && best.is_none_or(|(bd, _)| d < bd)
                {
                    best = Some((d, mi));
                }
            }
            if let Some((_, mi)) = best {
                owned[mi].push(j);
            }
        }
        owned
    }

    fn harmonize_brute_force(
        mut modis: Vec<Hotspot>,
        mut viirs: Vec<Hotspot>,
    ) -> Vec<HarmonizedHotspot> {
        remove_interscan_duplicates(&mut modis);
        remove_interscan_duplicates(&mut viirs);
        let modis = fleet::consolidate_modis_fleet(&modis);
        let viirs = fleet::consolidate_viirs_fleet(&viirs);

        let mut result = Vec::new();
        let mut viirs_matched = vec![false; viirs.len()];
        let owned = brute_force_owned(&modis, &viirs, &MatchingConfig::default());

        for (mi, m) in modis.iter().enumerate() {
            match viirs_pass_members(m, &owned[mi], &viirs, &MatchingConfig::default()) {
                Some(pass) => {
                    for &j in &pass {
                        viirs_matched[j] = true;
                    }
                    let members: Vec<&FleetObservation> = pass.iter().map(|&j| &viirs[j]).collect();
                    result.push(fuse_matched(m, &members, &UncertaintyParams::default()));
                }
                None => result.push(from_modis_only(m)),
            }
        }

        for (j, v) in viirs.iter().enumerate() {
            if !viirs_matched[j] {
                result.push(from_viirs_only(v, &UncertaintyParams::default()));
            }
        }

        result.sort_by_key(|a| a.acquired);
        result
    }

    fn assert_harmonized_eq(actual: &[HarmonizedHotspot], expected: &[HarmonizedHotspot]) {
        assert_eq!(actual.len(), expected.len());
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(a.lat, e.lat, "lat mismatch at index {i}");
            assert_eq!(a.lon, e.lon, "lon mismatch at index {i}");
            assert_eq!(a.frp, e.frp, "frp mismatch at index {i}");
            assert_eq!(
                a.frp_corrected, e.frp_corrected,
                "frp_corrected mismatch at index {i}"
            );
            assert_eq!(
                a.frp_lower, e.frp_lower,
                "frp_lower mismatch at index {i}"
            );
            assert_eq!(
                a.frp_upper, e.frp_upper,
                "frp_upper mismatch at index {i}"
            );
            assert_eq!(
                a.vza_factor, e.vza_factor,
                "vza_factor mismatch at index {i}"
            );
            assert_eq!(a.n_obs, e.n_obs, "n_obs mismatch at index {i}");
            assert_eq!(a.platforms, e.platforms, "platforms mismatch at index {i}");
            assert!(a.confidence == e.confidence, "confidence mismatch at {i}");
            assert_eq!(a.source, e.source, "source mismatch at index {i}");
            assert_eq!(a.acquired, e.acquired, "acquired mismatch at index {i}");
            assert_eq!(a.daynight, e.daynight, "daynight mismatch at index {i}");
        }
    }

    #[test]
    fn pair_inside_both_windows_matches() {
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            50.0,
            Confidence::High,
        )];
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.005,
            32.005,
            60,
            42.0,
            Confidence::Nominal,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].frp, 42.0);
        assert!(out[0].confidence == Confidence::High);
    }

    #[test]
    fn pair_3km_apart_does_not_match() {
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        )];
        // 0.027 deg latitude is ~3.00 km at any longitude
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.027,
            32.0,
            0,
            40.0,
            Confidence::High,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|h| h.source == HotspotSource::ModisOnly));
        assert!(out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
    }

    #[test]
    fn pair_190_minutes_apart_does_not_match() {
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        )];
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            MATCH_WINDOW_SECS + 600,
            40.0,
            Confidence::High,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|h| h.source == HotspotSource::ModisOnly));
        assert!(out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
    }

    #[test]
    fn single_viirs_cannot_serve_two_modis() {
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 10, 60.0, Confidence::Nominal),
        ];
        let viirs = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            5,
            45.0,
            Confidence::High,
        )];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 2);
        let matched = out
            .iter()
            .filter(|h| h.source == HotspotSource::Matched)
            .count();
        let modis_only = out
            .iter()
            .filter(|h| h.source == HotspotSource::ModisOnly)
            .count();
        assert_eq!(matched, 1);
        assert_eq!(modis_only, 1);
    }

    #[test]
    fn exact_window_boundary_matches_and_one_second_past_does_not() {
        let modis_before = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            10.0,
            Confidence::Low,
        )];
        let viirs_at_boundary = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            MATCH_WINDOW_SECS,
            20.0,
            Confidence::Nominal,
        )];
        let out_plus = harmonize(modis_before, viirs_at_boundary);
        assert_eq!(out_plus.len(), 1);
        assert_eq!(out_plus[0].source, HotspotSource::Matched);

        let modis_after = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            MATCH_WINDOW_SECS,
            10.0,
            Confidence::Low,
        )];
        let viirs_at_0 = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            0,
            20.0,
            Confidence::Nominal,
        )];
        let out_minus = harmonize(modis_after, viirs_at_0);
        assert_eq!(out_minus.len(), 1);
        assert_eq!(out_minus[0].source, HotspotSource::Matched);

        let modis_outside = vec![make_hotspot(
            Sensor::ModisTerra,
            39.0,
            32.0,
            0,
            10.0,
            Confidence::Low,
        )];
        let viirs_past_boundary = vec![make_hotspot(
            Sensor::ViirsSNPP,
            39.0,
            32.0,
            MATCH_WINDOW_SECS + 1,
            20.0,
            Confidence::Nominal,
        )];
        let out_outside = harmonize(modis_outside, viirs_past_boundary);
        assert_eq!(out_outside.len(), 2);
        assert!(out_outside.iter().all(|h| h.source != HotspotSource::Matched));
    }

    #[test]
    fn early_matched_viirs_does_not_trap_lo_for_later_modis() {
        // m0 at t=0 matches v1 at t=200 (loc A); v0 at t=50 (loc B) remains unmatched for m1 at t=100.
        // Then m2 at t=400 (loc C) still sees v1 inside its [100, 700] window, skips matched v1,
        // and matches v2 at t=450 (loc C).
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, 38.0, 30.0, 0, 10.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 100, 11.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, 40.0, 34.0, 400, 12.0, Confidence::Nominal),
        ];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 50, 20.0, Confidence::High),
            make_hotspot(Sensor::ViirsSNPP, 38.0, 30.0, 200, 21.0, Confidence::High),
            make_hotspot(Sensor::ViirsSNPP, 40.0, 34.0, 450, 22.0, Confidence::High),
        ];

        let expected = harmonize_brute_force(modis.clone(), viirs.clone());
        let actual = harmonize(modis, viirs);
        assert_harmonized_eq(&actual, &expected);
        assert_eq!(actual.len(), 3);
        assert!(actual.iter().all(|h| h.source == HotspotSource::Matched));
    }

    #[test]
    fn sliding_window_matches_brute_force_on_synthetic_dataset() {
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next_u64 = || -> u64 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };

        let mut modis = Vec::with_capacity(2_500);
        let mut viirs = Vec::with_capacity(3_500);

        let confs = [Confidence::Low, Confidence::Nominal, Confidence::High];

        for i in 0..2_500 {
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let cluster = (next_u64() % 40) as f64;
            let lat = 36.0 + (cluster * 0.15) + (r1 - 0.5) * 0.04;
            let lon = 27.0 + (cluster * 0.25) + (r2 - 0.5) * 0.04;
            let t = ((next_u64() % 20_000) as i64) - 500;
            let conf = confs[(next_u64() % 3) as usize];
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                t,
                10.0 + (i % 50) as f64,
                conf,
            ));
        }

        for i in 0..3_500 {
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let cluster = (next_u64() % 40) as f64;
            let lat = 36.0 + (cluster * 0.15) + (r1 - 0.5) * 0.04;
            let lon = 27.0 + (cluster * 0.25) + (r2 - 0.5) * 0.04;
            let t = ((next_u64() % 20_000) as i64) - 500;
            let conf = confs[(next_u64() % 3) as usize];
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat,
                lon,
                t,
                12.0 + (i % 50) as f64,
                conf,
            ));
        }

        // Dense clusters within ~2 km radius and a shared 300s window to stress H3 cell boundaries.
        let dense_centers = [
            (12.0, 15.0),
            (38.5, 31.2),
            (70.0, 24.5),
            (-15.3, -47.8),
        ];
        for i in 0..600 {
            let (clat, clon) = dense_centers[(next_u64() as usize) % dense_centers.len()];
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let lat = clat + (r1 - 0.5) * 0.018;
            let lon = clon + (r2 - 0.5) * (0.018 / clat.to_radians().cos());
            let t = 5_000 + (next_u64() % 280) as i64;
            let conf = confs[(next_u64() % 3) as usize];
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                t,
                15.0 + (i % 40) as f64,
                conf,
            ));
        }

        for i in 0..1_400 {
            let (clat, clon) = dense_centers[(next_u64() as usize) % dense_centers.len()];
            let r1 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let r2 = (next_u64() >> 32) as f64 / u32::MAX as f64;
            let lat = clat + (r1 - 0.5) * 0.018;
            let lon = clon + (r2 - 0.5) * (0.018 / clat.to_radians().cos());
            let t = 5_000 + (next_u64() % 280) as i64;
            let conf = confs[(next_u64() % 3) as usize];
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat,
                lon,
                t,
                18.0 + (i % 40) as f64,
                conf,
            ));
        }

        // Include edge cases near the antimeridian and high latitudes
        modis.push(make_hotspot(
            Sensor::ModisTerra,
            10.0,
            179.995,
            1_000,
            25.0,
            Confidence::Nominal,
        ));
        viirs.push(make_hotspot(
            Sensor::ViirsSNPP,
            10.0,
            -179.995,
            1_100,
            30.0,
            Confidence::High,
        ));
        modis.push(make_hotspot(
            Sensor::ModisTerra,
            89.99,
            10.0,
            2_000,
            25.0,
            Confidence::Nominal,
        ));
        viirs.push(make_hotspot(
            Sensor::ViirsSNPP,
            89.99,
            170.0,
            2_100,
            30.0,
            Confidence::High,
        ));

        let expected = harmonize_brute_force(modis.clone(), viirs.clone());
        let actual = harmonize(modis, viirs);
        assert_harmonized_eq(&actual, &expected);
        let matched_count = actual
            .iter()
            .filter(|h| h.source == HotspotSource::Matched)
            .count();
        let expected_matched_count = expected
            .iter()
            .filter(|h| h.source == HotspotSource::Matched)
            .count();
        assert_eq!(matched_count, expected_matched_count);
        assert!(matched_count > 400);
    }

    #[test]
    fn modis_only_with_vza_preserves_raw_frp_and_applies_factor() {
        let mut m = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
        m.vza = Some(45.0);

        let out = harmonize(vec![m], vec![]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ModisOnly);
        assert_eq!(out[0].frp, 100.0);
        let factor = out[0].vza_factor.expect("expected Some(vza_factor)");
        assert!(factor > 1.0);
        assert!(out[0].frp_corrected > out[0].frp);
        assert!((out[0].frp_corrected - (100.0 * factor)).abs() < 1e-12);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[0].platforms, vec![Sensor::ModisTerra]);
    }

    #[test]
    fn modis_only_without_vza_keeps_frp_equal_and_factor_none() {
        let m = make_hotspot(Sensor::ModisAqua, 39.0, 32.0, 0, 80.0, Confidence::Nominal);

        let out = harmonize(vec![m], vec![]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ModisOnly);
        assert_eq!(out[0].frp, 80.0);
        assert!((out[0].frp_corrected - 80.0).abs() < 1e-12);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[0].platforms, vec![Sensor::ModisAqua]);
    }

    #[test]
    fn matched_hotspot_has_n_obs_two_and_sorted_platforms() {
        let mut m = make_hotspot(Sensor::ModisAqua, 39.0, 32.0, 0, 90.0, Confidence::Nominal);
        m.vza = Some(45.0);
        let v = make_hotspot(Sensor::ViirsNOAA20, 39.002, 32.002, 30, 55.0, Confidence::High);

        let out = harmonize(vec![m], vec![v]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].frp, 55.0);
        assert_eq!(out[0].frp_corrected, 55.0);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(out[0].n_obs, 2);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ModisAqua, Sensor::ViirsNOAA20]
        );
    }

    #[test]
    fn viirs_only_has_n_obs_one_and_no_vza_factor() {
        let mut v = make_hotspot(Sensor::ViirsNOAA21, 39.0, 32.0, 0, 37.5, Confidence::High);
        v.vza = Some(30.0);

        let out = harmonize(vec![], vec![v]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ViirsOnly);
        assert_eq!(out[0].frp, 37.5);
        assert_eq!(out[0].frp_corrected, 37.5);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(out[0].n_obs, 1);
        assert_eq!(out[0].platforms, vec![Sensor::ViirsNOAA21]);
    }

    #[test]
    fn fleet_consolidates_before_modis_matching_so_no_viirs_leaks() {
        // NOAA-20 (High) at t=0 becomes the fleet representative; NOAA-21 (Nominal) at t=45m
        // joins the fleet. MODIS at t=120s is within 300s of the representative.
        let modis = vec![make_hotspot(
            Sensor::ModisTerra,
            -14.0,
            25.0,
            120,
            80.0,
            Confidence::Nominal,
        )];
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsNOAA20,
                -14.0,
                25.0,
                0,
                40.0,
                Confidence::High,
            ),
            make_hotspot(
                Sensor::ViirsNOAA21,
                -14.005,
                25.0,
                45 * 60,
                60.0,
                Confidence::Nominal,
            ),
        ];

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].n_obs, 3);
        assert!(!out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
        assert!((out[0].frp - 40.0).abs() < 1e-9);
        assert!((out[0].frp_corrected - 50.0).abs() < 1e-9);
        assert_eq!(out[0].vza_factor, None);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ModisTerra, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21]
        );
    }

    #[test]
    fn modis_and_viirs_two_hours_apart_match_as_one_fire_event() {
        // 120 minutes is a realistic separation close to the live FIRMS median (~158 min).
        let modis = vec![make_hotspot(
            Sensor::ModisAqua,
            39.0,
            32.0,
            0,
            65.0,
            Confidence::Nominal,
        )];
        let viirs = vec![make_hotspot(
            Sensor::ViirsNOAA20,
            39.003,
            32.002,
            120 * 60,
            48.0,
            Confidence::High,
        )];
        debug_assert!(haversine_km(39.0, 32.0, 39.003, 32.002) < 0.5);

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].n_obs, 2);
        assert!(out[0].platforms.contains(&Sensor::ModisAqua));
        assert!(out[0].platforms.contains(&Sensor::ViirsNOAA20));
        assert!(!out.iter().any(|h| h.source == HotspotSource::ModisOnly));
        assert!(!out.iter().any(|h| h.source == HotspotSource::ViirsOnly));
    }

    #[test]
    fn terra_and_aqua_150_min_apart_consolidate_to_single_modis_only() {
        let modis = vec![
            make_hotspot(
                Sensor::ModisTerra,
                39.0,
                32.0,
                0,
                50.0,
                Confidence::Nominal,
            ),
            make_hotspot(
                Sensor::ModisAqua,
                39.0,
                32.0,
                150 * 60,
                70.0,
                Confidence::High,
            ),
        ];

        let out = harmonize(modis, vec![]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::ModisOnly);
        assert_eq!(out[0].n_obs, 2);
        assert_eq!(
            out[0].platforms,
            vec![Sensor::ModisTerra, Sensor::ModisAqua]
        );
    }

    #[test]
    fn empty_inputs_produce_empty_output() {
        assert!(harmonize(vec![], vec![]).is_empty());
    }

    #[test]
    fn every_modis_detection_matched_at_three_sites() {
        let sites = [(36.0, 30.0), (37.0, 31.0), (38.0, 32.0)];
        let mut modis = Vec::new();
        let mut viirs = Vec::new();

        for (i, &(lat, lon)) in sites.iter().enumerate() {
            let i = i as i64;
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                i * 600,
                50.0,
                Confidence::Nominal,
            ));
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat + 0.003,
                lon,
                i * 600 + 300,
                40.0,
                Confidence::High,
            ));
        }

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|h| h.source == HotspotSource::Matched));
        assert!(out.iter().all(|h| h.n_obs == 2));
        assert!(out.windows(2).all(|w| w[0].acquired <= w[1].acquired));
    }

    #[test]
    fn no_detection_matched_when_viirs_is_5km_away() {
        let sites = [(36.0, 30.0), (37.0, 31.0), (38.0, 32.0)];
        let mut modis = Vec::new();
        let mut viirs = Vec::new();

        for (i, &(lat, lon)) in sites.iter().enumerate() {
            let i = i as i64;
            modis.push(make_hotspot(
                Sensor::ModisTerra,
                lat,
                lon,
                i * 600,
                50.0,
                Confidence::Nominal,
            ));
            viirs.push(make_hotspot(
                Sensor::ViirsSNPP,
                lat + 0.045,
                lon,
                i * 600 + 300,
                40.0,
                Confidence::High,
            ));
        }

        let out = harmonize(modis, viirs);
        assert_eq!(out.len(), 6);
        assert_eq!(
            out.iter()
                .filter(|h| h.source == HotspotSource::ModisOnly)
                .count(),
            3
        );
        assert_eq!(
            out.iter()
                .filter(|h| h.source == HotspotSource::ViirsOnly)
                .count(),
            3
        );
        assert!(!out.iter().any(|h| h.source == HotspotSource::Matched));
    }

    fn synthetic(
        seed: u64,
        n_modis: usize,
        n_viirs: usize,
        pixel_km: f64,
    ) -> (Vec<Hotspot>, Vec<Hotspot>) {
        let mut state = seed;
        let mut next = || -> u64 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 11
        };
        let confs = [Confidence::Low, Confidence::Nominal, Confidence::High];
        let mut make = |sensor: Sensor, i: usize| {
            let cluster = (next() % 25) as f64;
            let r1 = (next() % 10_000) as f64 / 10_000.0;
            let r2 = (next() % 10_000) as f64 / 10_000.0;
            let t = (next() % 6_000) as i64;
            let mut h = make_hotspot(
                sensor,
                36.0 + cluster * 0.2 + (r1 - 0.5) * 0.03,
                27.0 + cluster * 0.3 + (r2 - 0.5) * 0.03,
                t,
                5.0 + (i % 37) as f64,
                confs[(next() % 3) as usize],
            );
            h.scan = pixel_km;
            h.track = pixel_km;
            h.vza = Some((next() % 60) as f64);
            h
        };
        let modis = (0..n_modis)
            .map(|i| {
                make(
                    if i % 2 == 0 {
                        Sensor::ModisTerra
                    } else {
                        Sensor::ModisAqua
                    },
                    i,
                )
            })
            .collect();
        let viirs = (0..n_viirs).map(|i| make(Sensor::ViirsSNPP, i)).collect();
        (modis, viirs)
    }

    fn assert_links_valid(modis: &[Hotspot], viirs: &[Hotspot], out: &HarmonizeLinks) {
        let n_out = out.hotspots.len();
        assert_eq!(out.modis_links.len(), modis.len());
        assert_eq!(out.viirs_links.len(), viirs.len());
        assert_eq!(
            n_out,
            modis.len() + viirs.len()
                - out.sama_removed
                - out.static_masked
                - out.interscan_removed
                - out.fleet_merged
                - out.matched_pairs
                - out.viirs_aggregated
        );

        let mut modis_by_out: Vec<Vec<usize>> = vec![Vec::new(); n_out];
        let mut viirs_by_out: Vec<Vec<usize>> = vec![Vec::new(); n_out];
        for (i, link) in out.modis_links.iter().enumerate() {
            if let Some(k) = *link {
                assert!(k < n_out, "MODIS link {k} out of range {n_out}");
                modis_by_out[k].push(i);
            }
        }
        for (j, link) in out.viirs_links.iter().enumerate() {
            if let Some(k) = *link {
                assert!(k < n_out, "VIIRS link {k} out of range {n_out}");
                viirs_by_out[k].push(j);
            }
        }

        let mut matched = 0;
        for (k, h) in out.hotspots.iter().enumerate() {
            let (ms, vs) = (&modis_by_out[k], &viirs_by_out[k]);
            assert_eq!(h.n_obs as usize, ms.len() + vs.len(), "output {k} n_obs");
            for &i in ms {
                assert!(
                    h.platforms.contains(&modis[i].sensor),
                    "output {k} missing MODIS sensor"
                );
            }
            for &j in vs {
                assert!(
                    h.platforms.contains(&viirs[j].sensor),
                    "output {k} missing VIIRS sensor"
                );
            }
            match h.source {
                HotspotSource::Matched => {
                    matched += 1;
                    assert!(!ms.is_empty() && !vs.is_empty(), "output {k}");
                }
                HotspotSource::ModisOnly => {
                    assert!(!ms.is_empty() && vs.is_empty(), "output {k}");
                }
                HotspotSource::ViirsOnly => {
                    assert!(ms.is_empty() && !vs.is_empty(), "output {k}");
                }
            }
        }
        assert_eq!(matched, out.matched_pairs);
    }

    fn check_equivalence_and_links(modis: Vec<Hotspot>, viirs: Vec<Hotspot>) -> HarmonizeLinks {
        let expected = harmonize_brute_force(modis.clone(), viirs.clone());
        let plain = harmonize(modis.clone(), viirs.clone());
        let linked = harmonize_with_links(modis.clone(), viirs.clone());
        assert_harmonized_eq(&plain, &expected);
        assert_harmonized_eq(&linked.hotspots, &plain);
        assert_links_valid(&modis, &viirs, &linked);
        linked
    }

    #[test]
    fn with_links_matches_harmonize_on_empty_inputs() {
        let out = check_equivalence_and_links(Vec::new(), Vec::new());
        assert!(out.hotspots.is_empty());
        let (modis, _) = synthetic(1, 5, 0, 0.0);
        check_equivalence_and_links(modis, Vec::new());
        let (_, viirs) = synthetic(2, 0, 5, 0.0);
        check_equivalence_and_links(Vec::new(), viirs);
    }

    #[test]
    fn with_links_matches_harmonize_without_duplicates() {
        let (modis, viirs) = synthetic(7, 1_500, 2_000, 0.0);
        let out = check_equivalence_and_links(modis, viirs);
        assert_eq!(out.interscan_removed, 0);
        assert!(out.matched_pairs > 0);
    }

    #[test]
    fn with_links_matches_harmonize_with_interscan_duplicates() {
        for seed in [11, 12, 13] {
            let (modis, viirs) = synthetic(seed, 2_000, 2_500, 4.0);
            let out = check_equivalence_and_links(modis, viirs);
            assert!(out.interscan_removed > 0, "seed {seed} produced no duplicates");
            assert!(out.matched_pairs > 0, "seed {seed} produced no matches");
        }
    }

    #[test]
    fn with_links_on_hand_built_case() {
        // Two overlapping MODIS rows 2 s apart collapse into one, which then matches the VIIRS row.
        let mut m0 = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 10.0, Confidence::Low);
        let mut m1 = make_hotspot(Sensor::ModisTerra, 39.001, 32.0, 2, 30.0, Confidence::High);
        let far = make_hotspot(Sensor::ModisAqua, 41.0, 35.0, -500, 8.0, Confidence::Nominal);
        let v0 = make_hotspot(Sensor::ViirsSNPP, 39.0005, 32.0, 60, 12.0, Confidence::Nominal);
        let v1 = make_hotspot(Sensor::ViirsSNPP, 37.0, 30.0, 900, 4.0, Confidence::Low);
        m0.scan = 1.0;
        m0.track = 1.0;
        m1.scan = 1.0;
        m1.track = 1.0;

        let out = check_equivalence_and_links(vec![m0, m1, far], vec![v0, v1]);
        assert_eq!(out.interscan_removed, 1);
        assert_eq!(out.fleet_merged, 0);
        assert_eq!(out.matched_pairs, 1);
        assert_eq!(out.hotspots.len(), 3);
        assert_eq!(out.modis_links, vec![None, Some(1), Some(0)]);
        assert_eq!(out.viirs_links, vec![Some(1), Some(2)]);
        assert_eq!(out.hotspots[1].source, HotspotSource::Matched);
        assert_eq!(out.hotspots[2].source, HotspotSource::ViirsOnly);
    }

    #[test]
    fn with_links_tracks_fleet_consolidation() {
        // Terra and Aqua at the same fire merge in fleet consolidation, then match VIIRS.
        let terra = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 20.0, Confidence::Nominal);
        let aqua = make_hotspot(Sensor::ModisAqua, 39.002, 32.0, 1800, 25.0, Confidence::High);
        let snpp = make_hotspot(Sensor::ViirsSNPP, 39.001, 32.0, 2400, 18.0, Confidence::Nominal);

        let out = check_equivalence_and_links(vec![terra, aqua], vec![snpp]);
        assert_eq!(out.interscan_removed, 0);
        assert_eq!(out.fleet_merged, 1);
        assert_eq!(out.matched_pairs, 1);
        assert_eq!(out.hotspots.len(), 1);
        assert_eq!(out.modis_links, vec![Some(0), Some(0)]);
        assert_eq!(out.viirs_links, vec![Some(0)]);
        assert_eq!(out.hotspots[0].source, HotspotSource::Matched);
        assert_eq!(out.hotspots[0].n_obs, 3);
    }

    #[test]
    fn narrower_match_window_splits_a_matched_pair() {
        let modis = vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::High)];
        let viirs = vec![make_hotspot(Sensor::ViirsSNPP, 39.009, 32.0, 3600, 40.0, Confidence::High)];

        let default_out = harmonize(modis.clone(), viirs.clone());
        assert_eq!(default_out.len(), 1);
        assert_eq!(default_out[0].source, HotspotSource::Matched);

        let mut config = PipelineConfig::default();
        config.matching.window_secs = 600;
        let out = harmonize_with_config(modis, viirs, &config);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|h| h.source != HotspotSource::Matched));
    }

    #[test]
    fn stricter_dedup_time_diff_keeps_both_detections() {
        let mut a = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 10.0, Confidence::Low);
        let mut b = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 8, 20.0, Confidence::High);
        a.scan = 1.0;
        a.track = 1.0;
        b.scan = 1.0;
        b.track = 1.0;

        assert_eq!(harmonize(vec![a.clone(), b.clone()], vec![]).len(), 1);

        let mut config = PipelineConfig::default();
        config.dedup.max_time_diff_secs = 2;
        let links = harmonize_with_links_config(vec![a, b], vec![], &config);
        assert_eq!(links.interscan_removed, 0);
        // Still within the fleet radius/window, but same platform records never merge there.
        assert_eq!(links.hotspots.len(), 2);
    }

    #[test]
    fn larger_match_radius_matches_pair_five_km_apart() {
        let modis = vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::Nominal)];
        // 0.045 deg latitude is ~5.0 km
        let viirs = vec![make_hotspot(Sensor::ViirsSNPP, 39.045, 32.0, 0, 40.0, Confidence::High)];

        assert_eq!(harmonize(modis.clone(), viirs.clone()).len(), 2);

        let mut config = PipelineConfig::default();
        config.matching.radius_km = 6.0;
        let out = harmonize_with_config(modis, viirs, &config);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, HotspotSource::Matched);
    }

    #[test]
    fn identity_vza_coefficients_leave_frp_uncorrected() {
        let mut modis = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::High);
        modis.vza = Some(45.0);

        let default_out = harmonize(vec![modis.clone()], vec![]);
        assert!(default_out[0].frp_corrected > default_out[0].frp);

        let mut config = PipelineConfig::default();
        config.vza = crate::config::VzaConfig { a: 0.0, b: 0.0, c: 1.0 };
        let out = harmonize_with_config(vec![modis], vec![], &config);
        assert_eq!(out[0].frp_corrected, out[0].frp);
    }

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn reference_harmonize_with_links_config(
        mut modis: Vec<Hotspot>,
        mut viirs: Vec<Hotspot>,
        config: &PipelineConfig,
    ) -> HarmonizeLinks {
        let modis_input_len = modis.len();
        let viirs_input_len = viirs.len();

        let modis_origin = remove_interscan_duplicates_indexed_with(&mut modis, &config.dedup);
        let viirs_origin = remove_interscan_duplicates_indexed_with(&mut viirs, &config.dedup);
        let interscan_removed =
            (modis_input_len - modis.len()) + (viirs_input_len - viirs.len());

        let (modis_fleet, modis_groups) =
            fleet::consolidate_modis_fleet_indexed_with(&modis, &config.fleet, &config.vza);
        let (viirs_fleet, viirs_groups) =
            fleet::consolidate_viirs_fleet_indexed_with(&viirs, &config.fleet, &config.vza);
        let fleet_merged =
            (modis.len() - modis_fleet.len()) + (viirs.len() - viirs_fleet.len());

        let mut result = Vec::with_capacity(modis_fleet.len() + viirs_fleet.len());
        let mut modis_fleet_out = vec![0usize; modis_fleet.len()];
        let mut viirs_fleet_out: Vec<Option<usize>> = vec![None; viirs_fleet.len()];
        let mut matched_pairs = 0usize;
        let mut viirs_aggregated = 0usize;
        let mut simultaneous_result_idx = Vec::new();
        let owned = brute_force_owned(&modis_fleet, &viirs_fleet, &config.matching);

        for (mi, m) in modis_fleet.iter().enumerate() {
            modis_fleet_out[mi] = result.len();
            let Some(pass) = viirs_pass_members(m, &owned[mi], &viirs_fleet, &config.matching) else {
                result.push(from_modis_only_with(m, &config.vza, &config.uncertainty));
                continue;
            };
            let dt = (viirs_fleet[pass[0]].hotspot.acquired - m.hotspot.acquired)
                .num_seconds()
                .abs();
            if dt <= config.matching.simultaneous_window_secs {
                simultaneous_result_idx.push(result.len());
            }
            for &j in &pass {
                viirs_fleet_out[j] = Some(result.len());
            }
            matched_pairs += 1;
            viirs_aggregated += pass.len() - 1;
            let members: Vec<&FleetObservation> = pass.iter().map(|&j| &viirs_fleet[j]).collect();
            result.push(fuse_matched(m, &members, &config.uncertainty));
        }

        for (j, v) in viirs_fleet.iter().enumerate() {
            if viirs_fleet_out[j].is_none() {
                viirs_fleet_out[j] = Some(result.len());
                result.push(from_viirs_only(v, &config.uncertainty));
            }
        }

        let mut order: Vec<usize> = (0..result.len()).collect();
        order.sort_by_key(|&k| result[k].acquired);
        let mut final_pos = vec![0usize; result.len()];
        for (new_pos, &old_pos) in order.iter().enumerate() {
            final_pos[old_pos] = new_pos;
        }
        let mut slots: Vec<Option<HarmonizedHotspot>> = result.into_iter().map(Some).collect();
        let hotspots: Vec<HarmonizedHotspot> = order
            .iter()
            .map(|&k| slots[k].take().expect("each output taken once"))
            .collect();

        let mut modis_links = vec![None; modis_input_len];
        for (mi, members) in modis_groups.iter().enumerate() {
            let out = final_pos[modis_fleet_out[mi]];
            for &dedup_idx in members {
                modis_links[modis_origin[dedup_idx]] = Some(out);
            }
        }

        let mut viirs_links = vec![None; viirs_input_len];
        for (vj, members) in viirs_groups.iter().enumerate() {
            let out =
                final_pos[viirs_fleet_out[vj].expect("every VIIRS fleet group produces or joins an output")];
            for &dedup_idx in members {
                viirs_links[viirs_origin[dedup_idx]] = Some(out);
            }
        }

        let mut simultaneous_matched: Vec<usize> =
            simultaneous_result_idx.iter().map(|&k| final_pos[k]).collect();
        simultaneous_matched.sort_unstable();

        HarmonizeLinks {
            hotspots,
            modis_links,
            viirs_links,
            static_masked: 0,
            sama_removed: 0,
            interscan_removed,
            fleet_merged,
            matched_pairs,
            viirs_aggregated,
            simultaneous_matched,
        }
    }

    fn assert_links_bits_eq(actual: &HarmonizeLinks, expected: &HarmonizeLinks, ctx: &str) {
        assert_eq!(
            actual.interscan_removed, expected.interscan_removed,
            "{ctx}: interscan_removed"
        );
        assert_eq!(actual.fleet_merged, expected.fleet_merged, "{ctx}: fleet_merged");
        assert_eq!(actual.matched_pairs, expected.matched_pairs, "{ctx}: matched_pairs");
        assert_eq!(actual.static_masked, expected.static_masked, "{ctx}: static_masked");
        assert_eq!(actual.sama_removed, expected.sama_removed, "{ctx}: sama_removed");
        assert_eq!(
            actual.viirs_aggregated, expected.viirs_aggregated,
            "{ctx}: viirs_aggregated"
        );
        assert_eq!(
            actual.simultaneous_matched, expected.simultaneous_matched,
            "{ctx}: simultaneous_matched"
        );
        assert_eq!(actual.modis_links, expected.modis_links, "{ctx}: modis_links");
        assert_eq!(actual.viirs_links, expected.viirs_links, "{ctx}: viirs_links");
        assert_eq!(actual.hotspots.len(), expected.hotspots.len(), "{ctx}: len");

        for (i, (a, e)) in actual
            .hotspots
            .iter()
            .zip(expected.hotspots.iter())
            .enumerate()
        {
            assert_eq!(a.lat.to_bits(), e.lat.to_bits(), "{ctx} [{i}]: lat");
            assert_eq!(a.lon.to_bits(), e.lon.to_bits(), "{ctx} [{i}]: lon");
            assert_eq!(a.frp.to_bits(), e.frp.to_bits(), "{ctx} [{i}]: frp");
            assert_eq!(
                a.frp_corrected.to_bits(),
                e.frp_corrected.to_bits(),
                "{ctx} [{i}]: frp_corrected"
            );
            assert_eq!(
                a.frp_lower.to_bits(),
                e.frp_lower.to_bits(),
                "{ctx} [{i}]: frp_lower"
            );
            assert_eq!(
                a.frp_upper.to_bits(),
                e.frp_upper.to_bits(),
                "{ctx} [{i}]: frp_upper"
            );
            assert_eq!(
                a.vza_factor.map(f64::to_bits),
                e.vza_factor.map(f64::to_bits),
                "{ctx} [{i}]: vza_factor"
            );
            assert_eq!(a.n_obs, e.n_obs, "{ctx} [{i}]: n_obs");
            assert_eq!(a.platforms, e.platforms, "{ctx} [{i}]: platforms");
            assert_eq!(a.confidence, e.confidence, "{ctx} [{i}]: confidence");
            assert_eq!(a.source, e.source, "{ctx} [{i}]: source");
            assert_eq!(a.acquired, e.acquired, "{ctx} [{i}]: acquired");
            assert_eq!(a.daynight, e.daynight, "{ctx} [{i}]: daynight");
        }
    }

    fn synthetic_pipeline_dataset(
        seed: u64,
        n_modis: usize,
        n_viirs: usize,
    ) -> (Vec<Hotspot>, Vec<Hotspot>) {
        let mut rng = seed;
        let base = Utc.with_ymd_and_hms(2025, 7, 15, 12, 0, 0).unwrap();
        let confs = [Confidence::Low, Confidence::Nominal, Confidence::High];
        let modis_sensors = [Sensor::ModisTerra, Sensor::ModisAqua];
        let viirs_sensors = [Sensor::ViirsSNPP, Sensor::ViirsNOAA20, Sensor::ViirsNOAA21];
        let centers = [
            (39.0, 32.0),
            (-14.2, -52.8),
            (0.0, 179.995),
            (0.0, -179.995),
            (85.0, 10.0),
            (-85.0, -120.0),
        ];

        let mut make_row = |sensor: Sensor| {
            let (clat, clon) = centers[(xorshift64(&mut rng) as usize) % centers.len()];
            let exact = xorshift64(&mut rng).is_multiple_of(8);
            let lat = if exact {
                clat
            } else {
                let r1 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
                (clat + (r1 - 0.5) * 0.04).clamp(-89.9, 89.9)
            };
            let mut lon = if exact {
                clon
            } else {
                let r2 = (xorshift64(&mut rng) % 10_000) as f64 / 10_000.0;
                clon + (r2 - 0.5) * 0.04
            };
            if lon > 180.0 {
                lon -= 360.0;
            } else if lon < -180.0 {
                lon += 360.0;
            }

            let minute = (xorshift64(&mut rng) % 360) as i64;
            let burst_sec = ((xorshift64(&mut rng) % 4) as i64) * 3;
            let millis = ((xorshift64(&mut rng) % 4) as i64) * 250;
            let acquired = base + Duration::minutes(minute) + Duration::seconds(burst_sec) + Duration::milliseconds(millis);

            let mut h = make_hotspot(
                sensor,
                lat,
                lon,
                0,
                5.0 + (xorshift64(&mut rng) % 400) as f64 * 0.25,
                confs[(xorshift64(&mut rng) % 3) as usize],
            );
            h.acquired = acquired;
            h.daynight = if xorshift64(&mut rng).is_multiple_of(2) {
                DayNight::Day
            } else {
                DayNight::Night
            };
            h.scan = 0.5 + (xorshift64(&mut rng) % 4) as f64 * 0.6;
            h.track = 0.5 + (xorshift64(&mut rng) % 4) as f64 * 0.6;
            h.vza = Some((xorshift64(&mut rng) % 60) as f64);
            h
        };

        let mut modis: Vec<Hotspot> = (0..n_modis)
            .map(|i| make_row(modis_sensors[i % modis_sensors.len()]))
            .collect();
        let mut viirs: Vec<Hotspot> = (0..n_viirs)
            .map(|i| make_row(viirs_sensors[i % viirs_sensors.len()]))
            .collect();

        // Inject invalid coordinates (lat = 95.0) in both streams.
        for k in 0..6 {
            let mut m = make_hotspot(
                modis_sensors[k % 2],
                95.0,
                20.0 + (k % 2) as f64 * 0.001,
                (k % 3) as i64 * 600,
                20.0 + k as f64,
                confs[k % 3],
            );
            m.scan = 1.5;
            m.track = 1.5;
            modis.push(m);

            let mut v = make_hotspot(
                viirs_sensors[k % 3],
                95.0,
                20.0 + (k % 2) as f64 * 0.001,
                (k % 3) as i64 * 600 + 30,
                18.0 + k as f64,
                confs[(k + 1) % 3],
            );
            v.scan = 1.5;
            v.track = 1.5;
            viirs.push(v);
        }

        (modis, viirs)
    }

    #[test]
    fn timed_harmonize_with_links_is_bit_identical_to_reference_across_seeds_and_configs() {
        let mut large_radius = PipelineConfig::default();
        large_radius.fleet.radius_km = 10.0;
        large_radius.matching.radius_km = 10.0;

        let mut zero_radius = PipelineConfig::default();
        zero_radius.fleet.radius_km = 0.0;
        zero_radius.matching.radius_km = 0.0;

        let mut neg_dedup_ratio = PipelineConfig::default();
        neg_dedup_ratio.dedup.min_overlap_ratio = -0.1;

        let mut zero_window = PipelineConfig::default();
        zero_window.dedup.max_time_diff_secs = 0;
        zero_window.fleet.viirs_window_secs = 0;
        zero_window.fleet.modis_window_secs = 0;
        zero_window.matching.window_secs = 0;

        let mut configs = [
            ("default", PipelineConfig::default()),
            ("large_radius_10km", large_radius),
            ("zero_radius", zero_radius),
            ("neg_dedup_ratio", neg_dedup_ratio),
            ("zero_window", zero_window),
        ];
        // The reference models steps 1-4 only; step 0 is checked in its own tests.
        for (_, cfg) in &mut configs {
            cfg.sama.enabled = false;
            cfg.static_mask.enabled = false;
        }

        for seed in [0x9E37_79B9_7F4A_7C15_u64, 0xCAFE_BABE_1357_2468_u64] {
            let full = synthetic_pipeline_dataset(seed, 1_200, 1_800);
            let fallback = synthetic_pipeline_dataset(seed ^ 0x33CC, 300, 450);

            for (label, cfg) in &configs {
                let (modis, viirs) = if cfg.dedup.min_overlap_ratio < 0.0 {
                    &fallback
                } else {
                    &full
                };
                let actual = harmonize_with_links_config(modis.clone(), viirs.clone(), cfg);
                let expected =
                    reference_harmonize_with_links_config(modis.clone(), viirs.clone(), cfg);
                assert_links_bits_eq(&actual, &expected, &format!("{label} seed={seed}"));
            }
        }
    }

    #[test]
    fn calibration_is_not_applied_per_detection_across_latitudes() {
        let mut modis_tropics =
            make_hotspot(Sensor::ModisTerra, 10.0, 20.0, 0, 100.0, Confidence::High);
        modis_tropics.vza = Some(30.0);

        let mut modis_boreal =
            make_hotspot(Sensor::ModisTerra, 60.0, 90.0, 10, 100.0, Confidence::High);
        modis_boreal.vza = Some(30.0);

        let mut modis_for_match =
            make_hotspot(Sensor::ModisAqua, -10.0, 30.0, 20, 100.0, Confidence::Nominal);
        modis_for_match.vza = Some(30.0);
        let viirs_matched =
            make_hotspot(Sensor::ViirsSNPP, -10.002, 30.002, 80, 50.0, Confidence::High);

        let viirs_only =
            make_hotspot(Sensor::ViirsNOAA20, 62.0, 95.0, 120, 40.0, Confidence::High);

        let out = harmonize(
            vec![modis_tropics, modis_boreal, modis_for_match],
            vec![viirs_matched, viirs_only],
        );
        assert_eq!(out.len(), 4);

        let tropics = &out[0];
        let boreal = &out[1];
        let matched = &out[2];
        let v_only = &out[3];

        assert_eq!(tropics.source, HotspotSource::ModisOnly);
        assert_eq!(boreal.source, HotspotSource::ModisOnly);
        assert_eq!(tropics.frp, 100.0);
        assert_eq!(boreal.frp, 100.0);
        // Same sensor, scan/VZA, and raw FRP at tropical vs boreal latitudes must yield identical
        // per-detection frp_corrected (completeness k_c belongs only to pre-VIIRS calendar totals).
        assert_eq!(tropics.frp_corrected.to_bits(), boreal.frp_corrected.to_bits());

        let factor = tropics.vza_factor.unwrap();
        assert!((tropics.frp_corrected - 100.0 * factor).abs() < 1e-12);
        assert!(tropics.frp_corrected > tropics.frp);

        assert_eq!(matched.source, HotspotSource::Matched);
        assert_eq!(matched.frp_corrected, 50.0);

        assert_eq!(v_only.source, HotspotSource::ViirsOnly);
        assert_eq!(v_only.frp_corrected, 40.0);
    }

    #[test]
    fn disabled_calibration_does_not_change_per_detection_vza_correction() {
        let mut m_trop = make_hotspot(Sensor::ModisTerra, 10.0, 20.0, 0, 100.0, Confidence::High);
        m_trop.vza = Some(40.0);
        let mut m_boreal = make_hotspot(Sensor::ModisAqua, 60.0, 90.0, 60, 75.0, Confidence::Nominal);
        m_boreal.vza = Some(25.0);

        let cfg_disabled = PipelineConfig {
            calibration: crate::config::CalibrationConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let out_enabled = harmonize(vec![m_trop.clone(), m_boreal.clone()], vec![]);
        let out_disabled =
            harmonize_with_config(vec![m_trop.clone(), m_boreal.clone()], vec![], &cfg_disabled);
        assert_eq!(out_disabled.len(), 2);

        let expected_trop = vza::apply_vza_correction(m_trop.frp, vza::vza_factor(&m_trop));
        let expected_boreal = vza::apply_vza_correction(m_boreal.frp, vza::vza_factor(&m_boreal));

        assert_eq!(out_disabled[0].frp_corrected.to_bits(), expected_trop.to_bits());
        assert_eq!(out_disabled[1].frp_corrected.to_bits(), expected_boreal.to_bits());
        assert_eq!(out_enabled[0].frp_corrected.to_bits(), out_disabled[0].frp_corrected.to_bits());
        assert_eq!(out_enabled[1].frp_corrected.to_bits(), out_disabled[1].frp_corrected.to_bits());
    }

    #[test]
    fn modis_only_interval_widens_strictly_with_scan_vza() {
        let scans = [1.0, 1.5, 2.5, 4.0];
        let mut widths_equal_corrected = Vec::new();
        let mut rel_widths_fixed_raw = Vec::new();
        let mut prev_factor = 0.0;

        for scan in scans {
            let mut m_raw = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
            m_raw.scan = scan;
            let out_raw = harmonize(vec![m_raw], vec![]);
            assert_eq!(out_raw.len(), 1);
            let h = &out_raw[0];
            assert!(h.frp_lower <= h.frp_corrected && h.frp_corrected <= h.frp_upper);
            let factor = h.vza_factor.expect("scan > 0 yields vza_factor");
            assert!(factor > prev_factor);
            prev_factor = factor;
            rel_widths_fixed_raw.push((h.frp_upper - h.frp_lower) / h.frp_corrected);

            // Same footprint-scaled FRP across scan angles: frp_corrected = raw_frp * factor.
            let mut m_same_fire =
                make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0 / factor, Confidence::High);
            m_same_fire.scan = scan;
            let out_same = harmonize(vec![m_same_fire], vec![]);
            let hs = &out_same[0];
            assert!(hs.frp_lower <= hs.frp_corrected && hs.frp_corrected <= hs.frp_upper);
            widths_equal_corrected.push(hs.frp_upper - hs.frp_lower);
        }

        for pair in widths_equal_corrected.windows(2) {
            assert!(
                pair[1] > pair[0],
                "expected strictly increasing interval width with scan VZA, got {widths_equal_corrected:?}"
            );
        }
        for pair in rel_widths_fixed_raw.windows(2) {
            assert!(
                pair[1] > pair[0],
                "expected strictly increasing relative width with scan VZA, got {rel_widths_fixed_raw:?}"
            );
        }
    }

    #[test]
    fn matched_and_viirs_only_have_intervals_unaffected_by_vza() {
        let mut m_nadir = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
        m_nadir.scan = 1.0;
        let v_for_nadir = make_hotspot(Sensor::ViirsSNPP, 39.001, 32.001, 60, 50.0, Confidence::High);

        let mut m_edge = make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 100.0, Confidence::High);
        m_edge.scan = 4.0;
        let v_for_edge = make_hotspot(Sensor::ViirsSNPP, 39.001, 32.001, 60, 50.0, Confidence::High);

        let out_nadir = harmonize(vec![m_nadir], vec![v_for_nadir]);
        let out_edge = harmonize(vec![m_edge], vec![v_for_edge]);
        assert_eq!(out_nadir[0].source, HotspotSource::Matched);
        assert_eq!(out_edge[0].source, HotspotSource::Matched);
        assert!(out_nadir[0].frp_lower < out_nadir[0].frp_corrected);
        assert!(out_nadir[0].frp_corrected < out_nadir[0].frp_upper);
        assert_eq!(out_nadir[0].frp_lower.to_bits(), out_edge[0].frp_lower.to_bits());
        assert_eq!(out_nadir[0].frp_upper.to_bits(), out_edge[0].frp_upper.to_bits());

        let mut v_nadir = make_hotspot(Sensor::ViirsNOAA20, 39.0, 32.0, 0, 40.0, Confidence::Nominal);
        v_nadir.scan = 0.4;
        let mut v_edge = make_hotspot(Sensor::ViirsNOAA20, 39.0, 32.0, 0, 40.0, Confidence::Nominal);
        v_edge.scan = 1.6;
        v_edge.vza = Some(50.0);

        let vo_nadir = harmonize(vec![], vec![v_nadir]);
        let vo_edge = harmonize(vec![], vec![v_edge]);
        assert_eq!(vo_nadir[0].source, HotspotSource::ViirsOnly);
        assert!(vo_nadir[0].frp_lower < vo_nadir[0].frp_corrected);
        assert!(vo_nadir[0].frp_corrected < vo_nadir[0].frp_upper);
        assert_eq!(vo_nadir[0].frp_lower.to_bits(), vo_edge[0].frp_lower.to_bits());
        assert_eq!(vo_nadir[0].frp_upper.to_bits(), vo_edge[0].frp_upper.to_bits());
    }

    #[test]
    fn matched_interval_equals_viirs_only_interval_for_same_viirs_observation() {
        // MODIS contributes no FRP information to a Matched record (FRP comes from VIIRS), so the
        // interval must match the same VIIRS fleet observation processed on its own.
        let viirs_row =
            make_hotspot(Sensor::ViirsSNPP, 39.001, 32.001, 60, 50.0, Confidence::High);
        let matched = harmonize(
            vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High)],
            vec![viirs_row.clone()],
        );
        let viirs_only = harmonize(vec![], vec![viirs_row]);

        assert_eq!(matched[0].source, HotspotSource::Matched);
        assert_eq!(matched[0].n_obs, 2);
        assert_eq!(viirs_only[0].source, HotspotSource::ViirsOnly);
        assert_eq!(viirs_only[0].n_obs, 1);
        assert_eq!(matched[0].frp_corrected, viirs_only[0].frp_corrected);
        assert_eq!(
            matched[0].frp_lower.to_bits(),
            viirs_only[0].frp_lower.to_bits()
        );
        assert_eq!(
            matched[0].frp_upper.to_bits(),
            viirs_only[0].frp_upper.to_bits()
        );
    }

    #[test]
    fn matched_interval_follows_viirs_member_count_for_two_platform_viirs_fleet() {
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High),
            make_hotspot(Sensor::ModisAqua, 39.0, 32.0, 300, 90.0, Confidence::Nominal),
        ];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, 39.001, 32.0, 600, 50.0, Confidence::High),
            make_hotspot(Sensor::ViirsNOAA20, 39.001, 32.0, 900, 60.0, Confidence::High),
        ];

        let matched = harmonize(modis, viirs.clone());
        let viirs_only = harmonize(vec![], viirs);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].source, HotspotSource::Matched);
        assert_eq!(matched[0].n_obs, 4);
        assert_eq!(viirs_only.len(), 1);
        assert_eq!(viirs_only[0].source, HotspotSource::ViirsOnly);
        assert_eq!(viirs_only[0].n_obs, 2);
        assert_eq!(matched[0].frp_corrected, viirs_only[0].frp_corrected);
        assert_eq!(
            matched[0].frp_lower.to_bits(),
            viirs_only[0].frp_lower.to_bits()
        );
        assert_eq!(
            matched[0].frp_upper.to_bits(),
            viirs_only[0].frp_upper.to_bits()
        );
    }

    #[test]
    fn fuse_matched_interval_ignores_modis_member_count() {
        // Fleet groups cannot hold three MODIS members (only Terra and Aqua exist), so the
        // MODIS member count is exercised through a hand-built fleet observation.
        let params = UncertaintyParams::default();
        let v = FleetObservation {
            hotspot: make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 0, 50.0, Confidence::High),
            frp_mean: 50.0,
            frp_corrected_mean: 50.0,
            n_obs: 2,
            platforms: vec![Sensor::ViirsSNPP, Sensor::ViirsNOAA20],
        };
        let m = FleetObservation {
            hotspot: make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High),
            frp_mean: 80.0,
            frp_corrected_mean: 80.0,
            n_obs: 3,
            platforms: vec![Sensor::ModisTerra, Sensor::ModisAqua],
        };

        let matched = fuse_matched(&m, &[&v], &params);
        let viirs_only = from_viirs_only(&v, &params);

        assert_eq!(matched.n_obs, 5);
        assert_eq!(matched.frp_lower.to_bits(), viirs_only.frp_lower.to_bits());
        assert_eq!(matched.frp_upper.to_bits(), viirs_only.frp_upper.to_bits());
    }

    #[test]
    fn higher_n_obs_narrows_random_uncertainty_but_preserves_systematic() {
        // ViirsOnly (n_obs=1) vs Matched with 3 VIIRS + 1 MODIS (output n_obs=4) at identical
        // frp_corrected=50: only the three VIIRS observations enter the random term.
        let single_viirs = harmonize(
            vec![],
            vec![make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 0, 50.0, Confidence::High)],
        );
        let four_obs_matched = harmonize(
            vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 80.0, Confidence::High)],
            vec![
                make_hotspot(Sensor::ViirsSNPP, 39.001, 32.0, 10, 50.0, Confidence::High),
                make_hotspot(Sensor::ViirsNOAA20, 39.001, 32.0, 20, 50.0, Confidence::High),
                make_hotspot(Sensor::ViirsNOAA21, 39.001, 32.0, 30, 50.0, Confidence::High),
            ],
        );
        assert_eq!(single_viirs[0].n_obs, 1);
        assert_eq!(four_obs_matched[0].n_obs, 4);
        let w1 = single_viirs[0].frp_upper - single_viirs[0].frp_lower;
        let w4 = four_obs_matched[0].frp_upper - four_obs_matched[0].frp_lower;
        assert!(w4 < w1, "expected n_obs=4 width ({w4}) < n_obs=1 width ({w1})");
        assert!((w4 - w1 / 3f64.sqrt()).abs() < 1e-12);

        // ModisOnly: with zero random sensor/VZA sigma, systematic calibration sigma stays unscaled by n_obs.
        let obs1 = FleetObservation {
            hotspot: make_hotspot(Sensor::ModisTerra, 60.0, 20.0, 0, 100.0, Confidence::High),
            frp_mean: 100.0,
            frp_corrected_mean: 100.0,
            n_obs: 1,
            platforms: vec![Sensor::ModisTerra],
        };
        let mut obs4 = obs1.clone();
        obs4.n_obs = 4;
        let sys_only = UncertaintyParams {
            modis_sensor_sigma: 0.0,
            viirs_sensor_sigma: 0.0,
            vza_error_coeff: 0.0,
            ..UncertaintyParams::default()
        };
        let h1 = from_modis_only_with(&obs1, &VzaConfig::default(), &sys_only);
        let h4 = from_modis_only_with(&obs4, &VzaConfig::default(), &sys_only);
        assert!((h1.frp_lower - h4.frp_lower).abs() < 1e-12);
        assert!((h1.frp_upper - h4.frp_upper).abs() < 1e-12);
    }

    fn night(mut h: Hotspot) -> Hotspot {
        h.daynight = DayNight::Night;
        h
    }

    /// One detection on the 10th of every month of 2024, at `(lat, lon)`.
    fn year_round(sensor: Sensor, lat: f64, lon: f64) -> Vec<Hotspot> {
        (1..=12)
            .map(|m| {
                let mut h = make_hotspot(sensor, lat, lon, 0, 8.0, Confidence::Nominal);
                h.acquired = Utc.with_ymd_and_hms(2024, m, 10, 1, 0, 0).unwrap();
                h
            })
            .collect()
    }

    fn sama_on() -> PipelineConfig {
        let mut c = PipelineConfig::default();
        c.sama.enabled = true;
        c
    }

    fn prefilters_off() -> PipelineConfig {
        let mut c = PipelineConfig::default();
        c.sama.enabled = false;
        c.static_mask.enabled = false;
        c
    }

    #[test]
    fn sama_filter_drops_only_night_viirs_inside_the_box_and_reports_the_count() {
        let viirs = vec![
            night(make_hotspot(Sensor::ViirsSNPP, -15.0, -50.0, 0, 1.0, Confidence::Nominal)),
            make_hotspot(Sensor::ViirsSNPP, -15.1, -50.1, 0, 1.0, Confidence::Nominal),
            night(make_hotspot(Sensor::ViirsNOAA20, -12.0, 18.0, 0, 1.0, Confidence::Nominal)),
        ];
        let modis = vec![night(make_hotspot(Sensor::ModisAqua, -20.0, -45.0, 0, 9.0, Confidence::Nominal))];

        let on = harmonize_with_links_config(modis.clone(), viirs.clone(), &sama_on());
        assert_eq!(on.sama_removed, 1);
        assert_eq!(on.static_masked, 0);
        assert_eq!(on.hotspots.len(), 3);
        assert_eq!(on.viirs_links[0], None);
        assert!(on.viirs_links[1].is_some() && on.viirs_links[2].is_some());
        assert!(on.modis_links[0].is_some());
        assert_links_valid(&modis, &viirs, &on);

        let off = harmonize_with_links_config(modis, viirs, &PipelineConfig::default());
        assert_eq!(off.sama_removed, 0);
        assert_eq!(off.hotspots.len(), 4);
    }

    #[test]
    fn static_mask_drops_year_round_cells_and_keeps_seasonal_ones() {
        let flare = year_round(Sensor::ViirsSNPP, 4.80, 6.90);
        let mut seasonal = year_round(Sensor::ViirsSNPP, 9.50, 8.00);
        seasonal.truncate(7);
        let modis = year_round(Sensor::ModisAqua, 4.80, 6.90);
        let viirs: Vec<Hotspot> = flare.into_iter().chain(seasonal).collect();

        let on = harmonize_with_links_config(modis.clone(), viirs.clone(), &PipelineConfig::default());
        assert_eq!(on.static_masked, 24);
        assert_eq!(on.sama_removed, 0);
        assert_eq!(on.hotspots.len(), 7);
        assert!(on.modis_links.iter().all(Option::is_none));
        assert!(on.viirs_links[..12].iter().all(Option::is_none));
        assert!(on.viirs_links[12..].iter().all(Option::is_some));
        assert_links_valid(&modis, &viirs, &on);

        let mut cfg = PipelineConfig::default();
        cfg.static_mask.enabled = false;
        let off = harmonize_with_links_config(modis, viirs, &cfg);
        assert_eq!(off.static_masked, 0);
        // The 12 co-located MODIS/VIIRS flare pairs match: 12 matched + 7 seasonal VIIRS.
        assert_eq!(off.hotspots.len(), 19);
    }

    #[test]
    fn static_mask_threshold_edge_follows_config() {
        let mut viirs = year_round(Sensor::ViirsSNPP, 4.80, 6.90);
        viirs.truncate(8);
        let mut cfg = PipelineConfig::default();
        cfg.static_mask.min_active_months = 8;
        assert_eq!(harmonize_with_links_config(vec![], viirs.clone(), &cfg).static_masked, 8);
        cfg.static_mask.min_active_months = 9;
        assert_eq!(harmonize_with_links_config(vec![], viirs, &cfg).static_masked, 0);
    }

    #[test]
    fn supplied_mask_applies_to_a_sub_batch_that_alone_would_not_reach_the_threshold() {
        let year = year_round(Sensor::ViirsSNPP, 4.80, 6.90);
        let config = PipelineConfig::default();
        let mask = StaticMask::from_detections(&year, &config.static_mask);
        let march = vec![year[2].clone()];

        let alone = harmonize_with_links_config(vec![], march.clone(), &config);
        assert_eq!(alone.static_masked, 0);
        let shared = harmonize_with_links_masked(vec![], march.clone(), &config, Some(&mask));
        assert_eq!(shared.static_masked, 1);
        assert!(shared.hotspots.is_empty());
        let disabled = harmonize_with_links_masked(vec![], march, &prefilters_off(), Some(&mask));
        assert_eq!(disabled.static_masked, 0);
    }

    #[test]
    fn prefiltered_pipeline_equals_pipeline_on_manually_filtered_rows() {
        let (modis, viirs) = synthetic_pipeline_dataset(0x5EED_1234_ABCD_0001, 600, 900);
        let config = sama_on();
        let actual = harmonize_with_links_config(modis.clone(), viirs.clone(), &config);
        assert!(actual.sama_removed > 0);

        let kept = |rows: &[Hotspot]| -> Vec<usize> {
            (0..rows.len())
                .filter(|&i| !sama::is_sama_noise(&rows[i], &config.sama))
                .collect()
        };
        let (mk, vk) = (kept(&modis), kept(&viirs));
        let expected = harmonize_with_links_config(
            mk.iter().map(|&i| modis[i].clone()).collect(),
            vk.iter().map(|&i| viirs[i].clone()).collect(),
            &prefilters_off(),
        );
        assert_eq!(actual.sama_removed, modis.len() + viirs.len() - mk.len() - vk.len());
        assert_eq!(actual.hotspots.len(), expected.hotspots.len());
        assert_eq!(actual.interscan_removed, expected.interscan_removed);
        assert_eq!(actual.fleet_merged, expected.fleet_merged);
        assert_eq!(actual.matched_pairs, expected.matched_pairs);
        let remap = |links: &[Option<usize>], keep: &[usize], n: usize| {
            let mut full = vec![None; n];
            for (k, &i) in keep.iter().enumerate() {
                full[i] = links[k];
            }
            full
        };
        assert_eq!(actual.modis_links, remap(&expected.modis_links, &mk, modis.len()));
        assert_eq!(actual.viirs_links, remap(&expected.viirs_links, &vk, viirs.len()));
        assert_links_valid(&modis, &viirs, &actual);
    }

    fn near(lat_offset: f64) -> f64 {
        39.0 + lat_offset
    }

    #[test]
    fn footprint_radius_uses_half_diagonal_bounds_and_invalid_dimension_fallback() {
        let mut m = make_hotspot(
            Sensor::ModisTerra,
            near(0.0),
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        );
        let mut config = MatchingConfig::default();
        for (scan, track, expected) in [
            (1.0, 1.0, 0.75),
            (1.2, 1.6, 1.0),
            (4.8, 2.0, 2.0),
            (0.0, 1.0, 2.0),
            (1.0, 0.0, 2.0),
            (-1.0, 1.0, 2.0),
            (1.0, -1.0, 2.0),
            (f64::NAN, 1.0, 2.0),
            (1.0, f64::NAN, 2.0),
            (f64::INFINITY, 1.0, 2.0),
            (1.0, f64::INFINITY, 2.0),
        ] {
            m.scan = scan;
            m.track = track;
            assert!((footprint_radius_km(&m, &config) - expected).abs() < 1e-12);
        }
        m.scan = 1.0;
        m.track = 1.0;
        config.radius_km = 0.5;
        assert_eq!(footprint_radius_km(&m, &config), 0.5);
    }

    #[test]
    fn nadir_modis_footprint_keeps_distant_pass_pixel_viirs_only() {
        let mut m = make_hotspot(
            Sensor::ModisTerra,
            near(0.0),
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        );
        m.scan = 1.0;
        m.track = 1.0;
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0027),
                32.0,
                300,
                10.0,
                Confidence::Nominal,
            ),
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0135),
                32.0,
                300,
                20.0,
                Confidence::Nominal,
            ),
        ];

        let links = harmonize_with_links(vec![m], viirs);

        assert_eq!(links.hotspots.len(), 2);
        assert_eq!(links.hotspots[0].source, HotspotSource::Matched);
        assert_eq!(links.hotspots[0].frp_corrected, 10.0);
        assert_eq!(links.hotspots[0].n_obs, 2);
        assert_eq!(links.hotspots[1].source, HotspotSource::ViirsOnly);
        assert_eq!(links.hotspots[1].frp_corrected, 20.0);
        assert_eq!(links.matched_pairs, 1);
        assert_eq!(links.viirs_aggregated, 0);
        assert_eq!(links.viirs_links, vec![Some(0), Some(1)]);
    }

    #[test]
    fn scan_edge_modis_footprint_aggregates_distant_pass_pixel() {
        let mut m = make_hotspot(
            Sensor::ModisTerra,
            near(0.0),
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        );
        m.scan = 4.8;
        m.track = 2.0;
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0027),
                32.0,
                300,
                10.0,
                Confidence::Nominal,
            ),
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0135),
                32.0,
                300,
                20.0,
                Confidence::Nominal,
            ),
        ];

        let links = harmonize_with_links(vec![m], viirs);

        assert_eq!(links.hotspots.len(), 1);
        assert_eq!(links.hotspots[0].source, HotspotSource::Matched);
        assert_eq!(links.hotspots[0].frp_corrected, 30.0);
        assert_eq!(links.hotspots[0].n_obs, 3);
        assert_eq!(links.matched_pairs, 1);
        assert_eq!(links.viirs_aggregated, 1);
        assert_eq!(links.viirs_links, vec![Some(0); 2]);
    }

    #[test]
    fn unknown_modis_footprint_preserves_matching_radius_aggregation() {
        let mut m = make_hotspot(
            Sensor::ModisTerra,
            near(0.0),
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        );
        m.scan = 0.0;
        m.track = 0.0;
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0027),
                32.0,
                300,
                10.0,
                Confidence::Nominal,
            ),
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0135),
                32.0,
                300,
                20.0,
                Confidence::Nominal,
            ),
        ];

        let links = harmonize_with_links(vec![m], viirs);

        assert_eq!(links.hotspots.len(), 1);
        assert_eq!(links.hotspots[0].source, HotspotSource::Matched);
        assert_eq!(links.hotspots[0].frp_corrected, 30.0);
        assert_eq!(links.hotspots[0].n_obs, 3);
        assert_eq!(links.matched_pairs, 1);
        assert_eq!(links.viirs_aggregated, 1);
        assert_eq!(links.viirs_links, vec![Some(0); 2]);
    }

    #[test]
    fn viirs_pass_seed_can_lie_outside_modis_footprint() {
        let mut m = make_hotspot(
            Sensor::ModisTerra,
            near(0.0),
            32.0,
            0,
            50.0,
            Confidence::Nominal,
        );
        m.scan = 1.0;
        m.track = 1.0;
        let viirs = vec![
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0135),
                32.0,
                300,
                10.0,
                Confidence::Nominal,
            ),
            make_hotspot(
                Sensor::ViirsSNPP,
                near(0.0162),
                32.0,
                300,
                20.0,
                Confidence::Nominal,
            ),
        ];

        let links = harmonize_with_links(vec![m], viirs);

        assert_eq!(links.hotspots.len(), 2);
        assert_eq!(links.hotspots[0].source, HotspotSource::Matched);
        assert_eq!(links.hotspots[0].frp_corrected, 10.0);
        assert_eq!(links.hotspots[1].source, HotspotSource::ViirsOnly);
        assert_eq!(links.matched_pairs, 1);
        assert_eq!(links.viirs_aggregated, 0);
        assert_eq!(links.viirs_links, vec![Some(0), Some(1)]);
    }

    #[test]
    fn mixed_modis_footprints_conserve_viirs_frp_and_links() {
        let modis = [(0.0, 1.0, 1.0), (0.05, 2.0, 1.0)]
            .into_iter()
            .map(|(offset, scan, track)| {
                let mut m = make_hotspot(
                    Sensor::ModisTerra,
                    near(offset),
                    32.0,
                    0,
                    50.0,
                    Confidence::Nominal,
                );
                m.scan = scan;
                m.track = track;
                m
            })
            .collect::<Vec<_>>();
        let viirs = [
            (0.003, 1.0),
            (-0.006, 2.0),
            (0.0135, 4.0),
            (0.053, 8.0),
            (0.044, 16.0),
            (0.0635, 32.0),
        ]
        .into_iter()
        .map(|(offset, frp)| {
            make_hotspot(
                Sensor::ViirsSNPP,
                near(offset),
                32.0,
                300,
                frp,
                Confidence::Nominal,
            )
        })
        .collect::<Vec<_>>();
        let config = PipelineConfig::default();
        let expected = reference_harmonize_with_links_config(modis.clone(), viirs.clone(), &config);
        let brute_force = harmonize_brute_force(modis.clone(), viirs.clone());

        let links = harmonize_with_links(modis.clone(), viirs.clone());

        assert_links_bits_eq(&links, &expected, "mixed MODIS footprints");
        assert_harmonized_eq(&links.hotspots, &brute_force);
        assert_links_valid(&modis, &viirs, &links);
        assert_eq!(links.hotspots.len(), 4);
        assert_eq!(links.matched_pairs, 2);
        assert_eq!(links.viirs_aggregated, 2);
        assert!(links.viirs_links.iter().all(Option::is_some));
        let output_frp: f64 = links
            .hotspots
            .iter()
            .filter(|h| matches!(h.source, HotspotSource::Matched | HotspotSource::ViirsOnly))
            .map(|h| h.frp_corrected)
            .sum();
        let input_frp: f64 = viirs.iter().map(|h| h.frp).sum();
        assert!((output_frp - input_frp).abs() < 1e-12);
        for (j, expected_frp) in [(0, 3.0), (1, 3.0), (3, 24.0), (4, 24.0)] {
            let h = &links.hotspots[links.viirs_links[j].unwrap()];
            assert_eq!(h.source, HotspotSource::Matched);
            assert_eq!(h.frp_corrected, expected_frp);
        }
        for j in [2, 5] {
            let h = &links.hotspots[links.viirs_links[j].unwrap()];
            assert_eq!(h.source, HotspotSource::ViirsOnly);
            assert_eq!(h.frp_corrected, viirs[j].frp);
        }
    }

    #[test]
    fn viirs_platform_chain_longer_than_fleet_window_stays_split_through_the_pipeline() {
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 0, 10.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsNOAA20, 39.0, 32.0, 3000, 20.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsNOAA21, 39.0, 32.0, 6000, 30.0, Confidence::Nominal),
        ];

        let links = harmonize_with_links(vec![], viirs);

        assert_eq!(links.hotspots.len(), 2);
        assert_eq!(links.fleet_merged, 1);
        assert_eq!(links.hotspots[0].n_obs, 2);
        assert_eq!(links.hotspots[1].n_obs, 1);
    }

    #[test]
    fn matching_window_is_anchored_on_the_modis_observation_not_on_earlier_matches() {
        // v1 is 170 min after the MODIS record and matches; v2 is 170 min after v1 but 340 min
        // after MODIS, so it must not ride along on v1.
        let modis = vec![make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 40.0, Confidence::Nominal)];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 10_200, 10.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, 20_400, 20.0, Confidence::Nominal),
        ];

        let out = harmonize(modis, viirs);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].frp, 10.0);
        assert_eq!(out[1].source, HotspotSource::ViirsOnly);
        assert_eq!(out[1].frp, 20.0);
    }

    #[test]
    fn several_viirs_pixels_in_one_modis_footprint_become_one_matched_record_with_summed_frp() {
        let modis = vec![make_hotspot(Sensor::ModisTerra, near(0.0), 32.0, 0, 50.0, Confidence::Nominal)];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, near(0.004), 32.0, 600, 10.0, Confidence::Low),
            make_hotspot(Sensor::ViirsSNPP, near(0.008), 32.0, 600, 20.0, Confidence::High),
            make_hotspot(Sensor::ViirsSNPP, near(-0.004), 32.0, 600, 30.0, Confidence::Nominal),
        ];

        let links = harmonize_with_links(modis, viirs);

        assert_eq!(links.hotspots.len(), 1);
        let h = &links.hotspots[0];
        assert_eq!(h.source, HotspotSource::Matched);
        assert_eq!(h.frp, 60.0);
        assert_eq!(h.frp_corrected, 60.0);
        assert!(h.frp_lower < h.frp_corrected && h.frp_corrected < h.frp_upper);
        assert_eq!(h.n_obs, 4);
        assert_eq!(h.confidence, Confidence::High);
        assert_eq!(links.matched_pairs, 1);
        assert_eq!(links.viirs_aggregated, 2);
        assert_eq!(links.viirs_links, vec![Some(0); 3]);
        assert!(!links.hotspots.iter().any(|h| h.source == HotspotSource::ViirsOnly));
    }

    #[test]
    fn summed_viirs_pixels_have_a_narrower_relative_interval_than_a_single_pixel() {
        let single = harmonize(
            vec![make_hotspot(Sensor::ModisTerra, near(0.0), 32.0, 0, 50.0, Confidence::Nominal)],
            vec![make_hotspot(Sensor::ViirsSNPP, near(0.004), 32.0, 600, 30.0, Confidence::Nominal)],
        );
        let triple = harmonize(
            vec![make_hotspot(Sensor::ModisTerra, near(0.0), 32.0, 0, 50.0, Confidence::Nominal)],
            vec![
                make_hotspot(Sensor::ViirsSNPP, near(0.004), 32.0, 600, 10.0, Confidence::Nominal),
                make_hotspot(Sensor::ViirsSNPP, near(0.008), 32.0, 600, 10.0, Confidence::Nominal),
                make_hotspot(Sensor::ViirsSNPP, near(-0.004), 32.0, 600, 10.0, Confidence::Nominal),
            ],
        );

        let rel = |h: &HarmonizedHotspot| (h.frp_upper - h.frp_lower) / h.frp_corrected;
        assert_eq!(triple[0].frp, single[0].frp);
        assert!(rel(&triple[0]) < rel(&single[0]));
    }

    #[test]
    fn viirs_pixels_are_split_between_modis_centers_by_nearest_center() {
        // MODIS A and B are ~3 km apart. The middle pixel is inside 2 km of both but closer to B,
        // so it must be summed into B only.
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, near(0.0), 32.0, 0, 50.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, near(0.027), 32.0, 0, 60.0, Confidence::Nominal),
        ];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, near(0.004), 32.0, 300, 1.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsSNPP, near(0.008), 32.0, 300, 2.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsSNPP, near(0.015), 32.0, 300, 4.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsSNPP, near(0.031), 32.0, 300, 8.0, Confidence::Nominal),
        ];

        let out = harmonize(modis, viirs);

        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|h| h.source == HotspotSource::Matched));
        let mut frps: Vec<f64> = out.iter().map(|h| h.frp).collect();
        frps.sort_by(f64::total_cmp);
        assert_eq!(frps, vec![3.0, 12.0]);
    }

    #[test]
    fn viirs_pixels_of_a_different_pass_are_not_aggregated() {
        // The 2 h later pixel is still inside the 180 min matching window but belongs to another
        // overpass, so it stays a separate ViirsOnly record.
        let modis = vec![make_hotspot(Sensor::ModisTerra, near(0.0), 32.0, 0, 50.0, Confidence::Nominal)];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, near(0.004), 32.0, 300, 10.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsSNPP, near(0.008), 32.0, 300, 20.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsSNPP, near(0.004), 32.0, 7_500, 99.0, Confidence::Nominal),
        ];

        let out = harmonize(modis, viirs);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source, HotspotSource::Matched);
        assert_eq!(out[0].frp, 30.0);
        assert_eq!(out[1].source, HotspotSource::ViirsOnly);
        assert_eq!(out[1].frp, 99.0);
    }

    #[test]
    fn only_near_simultaneous_matches_are_listed_for_radiometric_estimation() {
        let modis = vec![
            make_hotspot(Sensor::ModisTerra, 39.0, 32.0, 0, 50.0, Confidence::Nominal),
            make_hotspot(Sensor::ModisTerra, 41.0, 35.0, 0, 50.0, Confidence::Nominal),
        ];
        let viirs = vec![
            make_hotspot(Sensor::ViirsSNPP, 39.0, 32.0, SIMULTANEOUS_WINDOW_SECS, 10.0, Confidence::Nominal),
            make_hotspot(Sensor::ViirsSNPP, 41.0, 35.0, SIMULTANEOUS_WINDOW_SECS + 1, 10.0, Confidence::Nominal),
        ];

        let links = harmonize_with_links(modis.clone(), viirs.clone());

        assert_eq!(links.matched_pairs, 2);
        assert_eq!(links.simultaneous_matched.len(), 1);
        let h = &links.hotspots[links.simultaneous_matched[0]];
        assert_eq!(h.source, HotspotSource::Matched);
        assert_eq!(h.lat, 39.0);

        let mut wider = PipelineConfig::default();
        wider.matching.simultaneous_window_secs = SIMULTANEOUS_WINDOW_SECS + 1;
        let links = harmonize_with_links_config(modis, viirs, &wider);
        assert_eq!(links.simultaneous_matched.len(), 2);
    }

    #[test]
    fn unequal_viirs_pixels_in_one_pass_have_wider_relative_interval_than_equal_pixels() {
        let offsets = [0.002, 0.004, 0.006, -0.004];
        let run = |frps: [f64; 4]| {
            let modis = vec![make_hotspot(
                Sensor::ModisTerra,
                near(0.0),
                32.0,
                0,
                50.0,
                Confidence::Nominal,
            )];
            let viirs = offsets
                .iter()
                .zip(frps)
                .map(|(&off, frp)| {
                    make_hotspot(
                        Sensor::ViirsSNPP,
                        near(off),
                        32.0,
                        600,
                        frp,
                        Confidence::Nominal,
                    )
                })
                .collect();
            harmonize(modis, viirs)
        };

        let unequal = run([100.0, 2.0, 2.0, 2.0]);
        let equal = run([26.5, 26.5, 26.5, 26.5]);

        assert_eq!(unequal.len(), 1);
        assert_eq!(equal.len(), 1);
        assert_eq!(unequal[0].source, HotspotSource::Matched);
        assert_eq!(equal[0].source, HotspotSource::Matched);
        assert_eq!(unequal[0].frp_corrected, 106.0);
        assert_eq!(equal[0].frp_corrected, 106.0);
        assert_eq!(unequal[0].n_obs, 5);
        assert_eq!(equal[0].n_obs, 5);

        let rel = |h: &HarmonizedHotspot| (h.frp_upper - h.frp_lower) / h.frp_corrected;
        assert!(rel(&unequal[0]) > rel(&equal[0]));
    }
}
