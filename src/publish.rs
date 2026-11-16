use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::model::Station;
use crate::sources::Source;

/// Width and height of a cell. Changing it changes every published path, so it needs a new `v` directory.
pub const CELL_DEGREES: f64 = 0.5;

#[derive(Serialize, Deserialize)]
pub struct SourceState {
    pub fetched: DateTime<Utc>,
    pub stations: Vec<Station>,
}

/// The cell holding a position, counted from 180° west and 90° south so both indices are never negative.
pub fn cell(lat: f64, lon: f64) -> (u32, u32) {
    (
        ((lon + 180.0) / CELL_DEGREES).floor() as u32,
        ((lat + 90.0) / CELL_DEGREES).floor() as u32,
    )
}

/// Writes every station with a recent enough price into its cell file under `out/v1/cells`, and
/// `out/v1/sources.json` describing the sources. Returns the number of stations and cells written.
pub fn publish(
    out: &Path,
    sources: &[&Source],
    states: &BTreeMap<String, SourceState>,
    now: DateTime<Utc>,
    max_price_age: Duration,
) -> Result<(usize, usize)> {
    let cutoff = now - max_price_age;
    let mut cells: BTreeMap<(u32, u32), Vec<Station>> = BTreeMap::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for src in sources {
        let Some(state) = states.get(&src.id) else {
            continue;
        };
        for station in &state.stations {
            let mut station = station.clone();
            station.prices.retain(|p| p.updated.is_none_or(|t| t >= cutoff));
            if station.prices.is_empty() {
                continue;
            }
            *counts.entry(&src.id).or_default() += 1;
            cells.entry(cell(station.lat, station.lon)).or_default().push(station);
        }
    }

    let root = out.join("v1");
    let fresh = root.join("cells.new");
    let current = root.join("cells");
    let old = root.join("cells.old");
    let _ = fs::remove_dir_all(&fresh);
    let total = cells.values().map(Vec::len).sum();
    for ((x, y), stations) in &cells {
        let dir = fresh.join(x.to_string());
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join(format!("{y}.json")),
            serde_json::to_vec(&json!({ "stations": stations }))?,
        )?;
    }
    fs::create_dir_all(&fresh)?;
    let _ = fs::remove_dir_all(&old);
    if current.exists() {
        fs::rename(&current, &old)?;
    }
    fs::rename(&fresh, &current).context("replacing cells")?;
    let _ = fs::remove_dir_all(&old);

    let described: Vec<_> = sources
        .iter()
        .map(|src| {
            json!({
                "id": src.id,
                "country": src.country,
                "attribution": src.attribution,
                "attribution_url": src.attribution_url,
                "licence": src.licence,
                "bounds": src.bounds,
                "fetched": states.get(&src.id).map(|s| s.fetched),
                "stations": counts.get(src.id.as_str()).copied().unwrap_or(0),
            })
        })
        .collect();
    let index = json!({
        "updated": now,
        "cell_degrees": CELL_DEGREES,
        "cells": "cells/{x}/{y}.json",
        "sources": described,
    });
    write_atomic(&root.join("sources.json"), &serde_json::to_vec_pretty(&index)?)?;
    Ok((total, cells.len()))
}

pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fuel, Price};
    use crate::sources::{Format, test_source};

    #[test]
    fn cells_count_from_the_south_west() {
        assert_eq!(cell(-90.0, -180.0), (0, 0));
        assert_eq!(cell(48.061, -2.69), (354, 276));
        assert_eq!(cell(0.49, 0.5), (361, 180));
    }

    #[test]
    fn writes_recent_prices_into_their_cells() {
        let dir = std::env::temp_dir().join(format!("ofp-publish-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let src = test_source(Format::France, [41.0, -5.5, 51.5, 10.0]);
        let now = Utc::now();
        let price = |days: i64| Price {
            fuel: Fuel::Diesel,
            price: 1.8,
            currency: "EUR".into(),
            updated: Some(now - Duration::days(days)),
        };
        let at = |id: &str, prices: Vec<Price>| Station {
            id: id.into(),
            source: "t".into(),
            lat: 48.061,
            lon: -2.69,
            name: None,
            brand: None,
            address: None,
            prices,
        };
        let states = BTreeMap::from([(
            "t".to_string(),
            SourceState {
                fetched: now,
                stations: vec![at("t:1", vec![price(1), price(40)]), at("t:2", vec![price(40)])],
            },
        )]);

        assert_eq!(
            publish(&dir, &[&src], &states, now, Duration::days(30)).unwrap(),
            (1, 1)
        );
        let cell: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("v1/cells/354/276.json")).unwrap()).unwrap();
        assert_eq!(cell["stations"][0]["id"], "t:1");
        assert_eq!(cell["stations"][0]["prices"].as_array().unwrap().len(), 1);
        let index: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("v1/sources.json")).unwrap()).unwrap();
        assert_eq!(index["sources"][0]["stations"], 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
