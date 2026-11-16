use anyhow::{Context, Result};
use chrono_tz::Europe::Paris;
use serde::Deserialize;

use super::{Source, station};
use crate::http::Http;
use crate::model::{Fuel, Price, Station, address, local_time, non_blank, title_case};

#[derive(Deserialize)]
struct Record {
    id: u64,
    geom: Option<Point>,
    adresse: Option<String>,
    ville: Option<String>,
    cp: Option<String>,
    #[serde(flatten)]
    fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Point {
    lat: f64,
    lon: f64,
}

const FUELS: [(&str, Fuel); 6] = [
    ("gazole", Fuel::Diesel),
    ("sp95", Fuel::Petrol95),
    ("e10", Fuel::E10),
    ("sp98", Fuel::Petrol98),
    ("e85", Fuel::E85),
    ("gplc", Fuel::Lpg),
];

pub fn fetch(src: &Source, http: &Http) -> Result<Vec<Station>> {
    parse(src, &http.get(&src.url, &src.headers)?)
}

fn parse(src: &Source, body: &[u8]) -> Result<Vec<Station>> {
    let records: Vec<Record> = serde_json::from_slice(body).context("France export")?;
    Ok(records
        .into_iter()
        .filter_map(|r| {
            let point = r.geom?;
            let mut s = station(src, r.id, point.lat, point.lon);
            s.address = address(
                r.adresse.as_deref().map(title_case),
                non_blank(r.cp.as_deref()),
                r.ville.as_deref().map(title_case),
            );
            s.prices = FUELS
                .iter()
                .filter_map(|(key, fuel)| {
                    let price = r.fields.get(&format!("{key}_prix"))?.as_f64()?;
                    let updated = r.fields.get(&format!("{key}_maj")).and_then(|v| v.as_str());
                    Some(Price {
                        fuel: *fuel,
                        price,
                        currency: "EUR".into(),
                        // The feed labels French local time as UTC.
                        updated: updated.and_then(|t| local_time(t.get(..19)?, "%Y-%m-%dT%H:%M:%S", Paris)),
                    })
                })
                .collect();
            Some(s)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{Format, test_source};

    #[test]
    fn reads_the_export_and_fixes_its_time_zone() {
        let body = br#"[{"id": 56580001, "geom": {"lon": -2.69, "lat": 48.061}, "adresse": "16 rue chateaubriand",
            "ville": "BREHAN", "cp": "56580", "gazole_prix": 2.109, "gazole_maj": "2026-11-12T06:55:20+00:00",
            "sp95_prix": null, "sp95_maj": null}]"#;
        let s = parse(&test_source(Format::France, [41.0, -5.5, 51.5, 10.0]), body)
            .unwrap()
            .remove(0);
        assert_eq!(s.id, "t:56580001");
        assert_eq!(s.address.as_deref(), Some("16 Rue Chateaubriand, 56580 Brehan"));
        assert_eq!(s.prices.len(), 1);
        assert_eq!(s.prices[0].updated.unwrap().to_rfc3339(), "2026-11-12T05:55:20+00:00");
    }
}
