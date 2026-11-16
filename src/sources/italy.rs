use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use chrono_tz::Europe::Rome;

use super::{Source, number, station};
use crate::http::Http;
use crate::model::{Fuel, Price, Station, address, local_time, non_blank, title_case};

fn fuel(name: &str) -> Option<Fuel> {
    Some(match name {
        "Benzina" => Fuel::Petrol95,
        "Gasolio" => Fuel::Diesel,
        "GPL" => Fuel::Lpg,
        "Metano" => Fuel::Cng,
        _ => return None,
    })
}

pub fn fetch(src: &Source, http: &Http) -> Result<Vec<Station>> {
    let prices_url = src
        .extra_urls
        .first()
        .context("italy needs the price list in extra_urls")?;
    let stations = http.get_text(&src.url, &src.headers)?;
    let prices = http.get_text(prices_url, &src.headers)?;
    Ok(parse(src, &stations, &prices))
}

/// Rows after the header line, split on `|`. Both files start with an extraction date line.
fn rows<'a>(text: &'a str, header_start: &str) -> impl Iterator<Item = Vec<&'a str>> {
    text.lines()
        .skip_while(move |l| !l.starts_with(header_start))
        .skip(1)
        .map(|l| l.split('|').collect())
}

fn parse(src: &Source, stations: &str, prices: &str) -> Vec<Station> {
    // Per station and fuel, self-service wins over attended, then the newer report.
    let mut best: HashMap<&str, BTreeMap<Fuel, (bool, Price)>> = HashMap::new();
    for r in rows(prices, "idImpianto") {
        let [id, name, price, is_self, updated] = r[..] else {
            continue;
        };
        let (Some(fuel), Some(price)) = (fuel(name), number(price)) else {
            continue;
        };
        let candidate = (
            is_self == "1",
            Price {
                fuel,
                price,
                currency: "EUR".into(),
                updated: local_time(updated, "%d/%m/%Y %H:%M:%S", Rome),
            },
        );
        let slot = best.entry(id).or_default();
        let better = slot.get(&fuel).is_none_or(|(self_service, current)| {
            (candidate.0, candidate.1.updated) > (*self_service, current.updated)
        });
        if better {
            slot.insert(fuel, candidate);
        }
    }
    rows(stations, "idImpianto")
        .filter_map(|r| {
            let n = r.len();
            if n < 10 {
                return None;
            }
            let id = r[0];
            let mut s = station(src, id, number(r[n - 2])?, number(r[n - 1])?);
            // Some names contain the separator, which shifts the middle columns. Only the ends are reliable then.
            if n == 10 {
                s.brand = non_blank(Some(r[2])).filter(|b| b != "Pompe Bianche");
                s.name = non_blank(Some(r[4])).map(|n| title_case(&n));
            }
            s.address = address(
                non_blank(Some(r[n - 5])).map(|a| title_case(&a)),
                None,
                non_blank(Some(r[n - 4])).map(|t| format!("{} ({})", title_case(&t), r[n - 3].trim())),
            );
            s.prices = best.remove(id)?.into_values().map(|(_, p)| p).collect();
            Some(s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{Format, test_source};

    #[test]
    fn prefers_self_service_and_survives_separators_in_names() {
        let stations = "Estrazione del 2026-11-15\n\
            idImpianto|Gestore|Bandiera|Tipo Impianto|Nome Impianto|Indirizzo|Comune|Provincia|Latitudine|Longitudine\n\
            59183|ENIMOOV S.P.A.|Agip Eni|Stradale|19829 AGRIGENTO|SS.189 KM. 64|AGRIGENTO|AG|37.333935|13.595533\n\
            23316|SANTORO|Pompe Bianche|Stradale|SANTORO | gestori.prezzibenzina.it|CONTRADA FIUMARA 11|CARIFE|AV|41.0186|15.1802\n";
        let prices = "Estrazione del 2026-11-15\n\
            idImpianto|descCarburante|prezzo|isSelf|dtComu\n\
            59183|Benzina|1.899|0|11/11/2026 14:00:12\n\
            59183|Benzina|1.799|1|01/11/2026 08:00:00\n\
            59183|Blue Diesel|1.999|1|11/11/2026 14:00:12\n\
            23316|Gasolio|1.699|1|11/11/2026 14:00:12\n";
        let out = parse(&test_source(Format::Italy, [35.0, 6.0, 47.5, 19.0]), stations, prices);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].brand.as_deref(), Some("Agip Eni"));
        assert_eq!(out[0].prices.len(), 1);
        assert_eq!(out[0].prices[0].price, 1.799);
        assert_eq!(out[1].brand, None);
        assert_eq!(out[1].address.as_deref(), Some("Contrada Fiumara 11, Carife (AV)"));
    }
}
