use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono_tz::Europe::Madrid;
use serde::Deserialize;

use super::{Source, number, station};
use crate::http::Http;
use crate::model::{Fuel, Price, Station, address, local_time, non_blank, title_case};

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "Fecha")]
    date: String,
    #[serde(rename = "ListaEESSPrecio")]
    stations: Vec<BTreeMap<String, String>>,
}

const FUELS: [(&str, Fuel); 7] = [
    ("Precio Gasoleo A", Fuel::Diesel),
    ("Precio Gasolina 95 E5", Fuel::Petrol95),
    ("Precio Gasolina 95 E10", Fuel::E10),
    ("Precio Gasolina 98 E5", Fuel::Petrol98),
    ("Precio Gasolina 95 E85", Fuel::E85),
    ("Precio Gases licuados del petróleo", Fuel::Lpg),
    ("Precio Gas Natural Comprimido", Fuel::Cng),
];

pub fn fetch(src: &Source, http: &Http) -> Result<Vec<Station>> {
    parse(src, &http.get(&src.url, &src.headers)?)
}

fn parse(src: &Source, body: &[u8]) -> Result<Vec<Station>> {
    let answer: Answer = serde_json::from_slice(body).context("Spanish station list")?;
    let updated = local_time(&answer.date, "%d/%m/%Y %H:%M:%S", Madrid);
    Ok(answer
        .stations
        .into_iter()
        // Stations selling only to members, such as cooperatives, are marked R.
        .filter(|s| s.get("Tipo Venta").is_none_or(|t| t != "R"))
        .filter_map(|f| {
            let field = |k: &str| f.get(k).map(String::as_str);
            let mut s = station(
                src,
                field("IDEESS")?,
                number(field("Latitud")?)?,
                number(field("Longitud (WGS84)")?)?,
            );
            s.brand = non_blank(field("Rótulo")).map(|b| title_case(&b));
            s.address = address(
                non_blank(field("Dirección")).map(|a| title_case(&a)),
                non_blank(field("C.P.")),
                non_blank(field("Localidad")).map(|l| title_case(&l)),
            );
            s.prices = FUELS
                .iter()
                .filter_map(|(key, fuel)| {
                    Some(Price {
                        fuel: *fuel,
                        price: number(field(key)?)?,
                        currency: "EUR".into(),
                        updated,
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
    fn reads_the_list_with_decimal_commas_and_skips_members_only_stations() {
        let body = r#"{"Fecha":"16/11/2026 22:57:44","ListaEESSPrecio":[
            {"IDEESS":"4375","Latitud":"39,211417","Longitud (WGS84)":"-1,539167","Rótulo":"REPSOL","Dirección":"AVENIDA CASTILLA, 26",
             "C.P.":"02250","Localidad":"ABENGIBRE","Precio Gasoleo A":"1,459","Precio Gasolina 95 E5":"","Tipo Venta":"P"},
            {"IDEESS":"9","Latitud":"39,2","Longitud (WGS84)":"-1,5","Precio Gasoleo A":"1,2","Tipo Venta":"R"}]}"#;
        let stations = parse(&test_source(Format::Spain, [27.0, -18.5, 44.5, 4.5]), body.as_bytes()).unwrap();
        assert_eq!(stations.len(), 1);
        let s = &stations[0];
        assert_eq!((s.lat, s.lon), (39.211417, -1.539167));
        assert_eq!(s.brand.as_deref(), Some("Repsol"));
        assert_eq!(s.prices.len(), 1);
        assert_eq!(s.prices[0].price, 1.459);
        assert_eq!(s.prices[0].updated.unwrap().to_rfc3339(), "2026-11-16T21:57:44+00:00");
    }
}
