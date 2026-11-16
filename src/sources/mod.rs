mod france;
mod italy;
mod mexico;
mod portugal;
mod spain;

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Deserialize;

use crate::http::Http;
use crate::model::Station;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// The prix-carburants instant feed as a JSON export.
    France,
    /// The ministry's whole-country station list.
    Spain,
    /// The DGEG station search, fetched per district. `url` holds a `{district}` placeholder.
    Portugal,
    /// MIMIT's station register at `url` and its price list as the first of `extra_urls`.
    Italy,
    /// The CNE's places at `url` and its prices as the first of `extra_urls`.
    Mexico,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub id: String,
    pub country: String,
    pub format: Format,
    pub url: String,
    #[serde(default)]
    pub extra_urls: Vec<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub poll_secs: u64,
    pub licence: String,
    pub attribution: String,
    pub attribution_url: String,
    /// South, west, north, east. Stations outside are dropped, after trying their coordinates swapped.
    pub bounds: [f64; 4],
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Source {
    fn contains(&self, lat: f64, lon: f64) -> bool {
        let [s, w, n, e] = self.bounds;
        (s..=n).contains(&lat) && (w..=e).contains(&lon)
    }
}

pub struct Fetched {
    pub stations: Vec<Station>,
    pub swapped: usize,
    pub dropped: usize,
}

pub fn fetch(src: &Source, http: &Http) -> Result<Fetched> {
    let stations = match src.format {
        Format::France => france::fetch(src, http)?,
        Format::Spain => spain::fetch(src, http)?,
        Format::Portugal => portugal::fetch(src, http)?,
        Format::Italy => italy::fetch(src, http)?,
        Format::Mexico => mexico::fetch(src, http)?,
    };
    Ok(clean(src, stations))
}

fn clean(src: &Source, stations: Vec<Station>) -> Fetched {
    let (mut swapped, mut dropped) = (0, 0);
    let stations = stations
        .into_iter()
        .filter_map(|mut s| {
            s.prices.retain(|p| p.price > 0.0 && p.price.is_finite());
            if s.prices.is_empty() {
                return None;
            }
            if !src.contains(s.lat, s.lon) {
                if src.contains(s.lon, s.lat) {
                    (s.lat, s.lon) = (s.lon, s.lat);
                    swapped += 1;
                } else {
                    dropped += 1;
                    return None;
                }
            }
            Some(s)
        })
        .collect();
    Fetched {
        stations,
        swapped,
        dropped,
    }
}

/// A station of `src` with the source's id prefix.
fn station(src: &Source, id: impl std::fmt::Display, lat: f64, lon: f64) -> Station {
    Station {
        id: format!("{}:{id}", src.id),
        source: src.id.clone(),
        lat,
        lon,
        name: None,
        brand: None,
        address: None,
        prices: Vec::new(),
    }
}

fn number(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

#[cfg(test)]
pub(crate) fn test_source(format: Format, bounds: [f64; 4]) -> Source {
    Source {
        id: "t".into(),
        country: "XX".into(),
        format,
        url: String::new(),
        extra_urls: Vec::new(),
        headers: BTreeMap::new(),
        poll_secs: 60,
        licence: "test".into(),
        attribution: "test".into(),
        attribution_url: "https://example.org".into(),
        bounds,
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fuel, Price};

    #[test]
    fn swaps_reversed_coordinates_and_drops_strays() {
        let src = test_source(Format::Spain, [27.0, -18.5, 44.5, 4.5]);
        let priced = |lat, lon| {
            let mut s = station(&src, 1, lat, lon);
            s.prices.push(Price {
                fuel: Fuel::Diesel,
                price: 1.5,
                currency: "EUR".into(),
                updated: None,
            });
            s
        };
        let out = clean(
            &src,
            vec![
                priced(40.4, -3.7),
                priced(-3.7, 40.4),
                priced(0.0, 0.0),
                station(&src, 2, 40.0, -3.0),
            ],
        );
        assert_eq!(out.stations.len(), 2);
        assert_eq!((out.stations[1].lat, out.stations[1].lon), (40.4, -3.7));
        assert_eq!((out.swapped, out.dropped), (1, 1));
    }
}
