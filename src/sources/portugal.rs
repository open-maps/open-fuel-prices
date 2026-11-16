use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono_tz::Europe::Lisbon;
use serde::Deserialize;

use super::{Source, number, station};
use crate::http::Http;
use crate::model::{Fuel, Price, Station, address, local_time, non_blank, title_case};

/// Mainland districts by the DGEG's ids. The islands have regulated prices and are not in the search.
const DISTRICTS: std::ops::RangeInclusive<u32> = 1..=18;

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "resultado", default)]
    rows: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "Id")]
    id: u64,
    #[serde(rename = "Nome")]
    name: Option<String>,
    #[serde(rename = "Marca")]
    brand: Option<String>,
    #[serde(rename = "Preco")]
    price: Option<String>,
    #[serde(rename = "Combustivel")]
    fuel: Option<String>,
    #[serde(rename = "DataAtualizacao")]
    updated: Option<String>,
    #[serde(rename = "Morada")]
    street: Option<String>,
    #[serde(rename = "Localidade")]
    town: Option<String>,
    #[serde(rename = "CodPostal")]
    postcode: Option<String>,
    #[serde(rename = "Latitude")]
    lat: Option<f64>,
    #[serde(rename = "Longitude")]
    lon: Option<f64>,
}

fn fuel(name: &str) -> Option<Fuel> {
    Some(match name {
        "Gasóleo simples" => Fuel::Diesel,
        "Gasolina simples 95" => Fuel::Petrol95,
        "Gasolina 98" => Fuel::Petrol98,
        "GPL Auto" => Fuel::Lpg,
        "GNC (gás natural comprimido) - €/kg" => Fuel::Cng,
        _ => return None,
    })
}

pub fn fetch(src: &Source, http: &Http) -> Result<Vec<Station>> {
    let mut rows = Vec::new();
    for district in DISTRICTS {
        let url = src.url.replace("{district}", &district.to_string());
        let answer: Answer =
            serde_json::from_slice(&http.get(&url, &src.headers)?).with_context(|| format!("district {district}"))?;
        rows.extend(answer.rows);
    }
    Ok(parse(src, rows))
}

fn parse(src: &Source, rows: Vec<Row>) -> Vec<Station> {
    let mut by_station: BTreeMap<u64, Vec<Row>> = BTreeMap::new();
    for row in rows {
        by_station.entry(row.id).or_default().push(row);
    }
    by_station
        .into_iter()
        .filter_map(|(id, rows)| {
            let first = &rows[0];
            let mut s = station(src, id, first.lat?, first.lon?);
            s.name = non_blank(first.name.as_deref());
            s.brand = non_blank(first.brand.as_deref()).map(|b| title_case(&b));
            s.address = address(
                non_blank(first.street.as_deref()),
                non_blank(first.postcode.as_deref()),
                non_blank(first.town.as_deref()),
            );
            s.prices = rows
                .iter()
                .filter_map(|r| {
                    Some(Price {
                        fuel: fuel(r.fuel.as_deref()?)?,
                        price: number(r.price.as_deref()?.split(' ').next()?)?,
                        currency: "EUR".into(),
                        updated: r
                            .updated
                            .as_deref()
                            .and_then(|t| local_time(t, "%Y-%m-%d %H:%M", Lisbon)),
                    })
                })
                .collect();
            Some(s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{Format, test_source};

    #[test]
    fn joins_a_stations_rows() {
        let body = r#"{"resultado":[
            {"Id":94719,"Nome":"PAGPL","Preco":"0,899 €","Marca":"GALP","Combustivel":"GPL Auto","DataAtualizacao":"2026-07-13 16:40",
             "Morada":"RETA DA GRANJA","Localidade":"Algueirão","CodPostal":"2725-056","Latitude":38.82075,"Longitude":-9.344724},
            {"Id":94719,"Preco":"1,659 €","Combustivel":"Gasóleo simples","DataAtualizacao":"2026-07-13 16:40","Latitude":38.82075,"Longitude":-9.344724},
            {"Id":94719,"Preco":"1,9 €","Combustivel":"Gasóleo especial","Latitude":38.82075,"Longitude":-9.344724}]}"#;
        let answer: Answer = serde_json::from_str(body).unwrap();
        let s = parse(&test_source(Format::Portugal, [36.9, -9.6, 42.2, -6.1]), answer.rows).remove(0);
        assert_eq!(s.id, "t:94719");
        assert_eq!(s.brand.as_deref(), Some("Galp"));
        assert_eq!(s.address.as_deref(), Some("RETA DA GRANJA, 2725-056 Algueirão"));
        assert_eq!(
            s.prices.iter().map(|p| p.fuel).collect::<Vec<_>>(),
            [Fuel::Lpg, Fuel::Diesel]
        );
        assert_eq!(s.prices[0].updated.unwrap().to_rfc3339(), "2026-07-13T15:40:00+00:00");
    }
}
