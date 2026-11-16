use std::collections::HashMap;

use anyhow::{Context, Result};

use super::{Source, number, station};
use crate::http::Http;
use crate::model::{Fuel, Price, Station, non_blank};

fn fuel(kind: &str) -> Option<Fuel> {
    Some(match kind {
        "regular" => Fuel::Regular,
        "premium" => Fuel::Premium,
        "diesel" => Fuel::Diesel,
        _ => return None,
    })
}

pub fn fetch(src: &Source, http: &Http) -> Result<Vec<Station>> {
    let prices_url = src
        .extra_urls
        .first()
        .context("mexico needs the prices in extra_urls")?;
    let places = http.get_text(&src.url, &src.headers)?;
    let prices = http.get_text(prices_url, &src.headers)?;
    parse(src, &places, &prices)
}

fn document(text: &str) -> Result<roxmltree::Document<'_>> {
    roxmltree::Document::parse(text.trim_start_matches('\u{feff}')).context("invalid XML")
}

fn places<'a, 'i>(doc: &'a roxmltree::Document<'i>) -> impl Iterator<Item = (&'a str, roxmltree::Node<'a, 'i>)> {
    doc.root_element()
        .children()
        .filter(|n| n.has_tag_name("place"))
        .filter_map(|n| Some((n.attribute("place_id")?, n)))
}

fn child_text<'a>(node: roxmltree::Node<'a, '_>, path: &[&str]) -> Option<&'a str> {
    let mut cur = node;
    for name in path {
        cur = cur.children().find(|c| c.has_tag_name(*name))?;
    }
    cur.text()
}

/// The CNE gives no report times, so prices carry none.
fn parse(src: &Source, places_xml: &str, prices_xml: &str) -> Result<Vec<Station>> {
    let prices_doc = document(prices_xml)?;
    let mut prices: HashMap<&str, Vec<Price>> = HashMap::new();
    for (id, place) in places(&prices_doc) {
        let entry = prices.entry(id).or_default();
        for p in place.children().filter(|c| c.has_tag_name("gas_price")) {
            let (Some(fuel), Some(price)) = (p.attribute("type").and_then(fuel), p.text().and_then(number)) else {
                continue;
            };
            entry.retain(|e| e.fuel != fuel);
            entry.push(Price {
                fuel,
                price,
                currency: "MXN".into(),
                updated: None,
            });
        }
    }
    let places_doc = document(places_xml)?;
    Ok(places(&places_doc)
        .filter_map(|(id, place)| {
            let lon = number(child_text(place, &["location", "x"])?)?;
            let lat = number(child_text(place, &["location", "y"])?)?;
            let mut s = station(src, id, lat, lon);
            s.name = non_blank(child_text(place, &["name"]));
            s.prices = prices.remove(id)?;
            Some(s)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{Format, test_source};

    #[test]
    fn joins_places_and_prices() {
        let places = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?><places>\
            <place place_id=\"2039\"><name>ESTACION HIPODROMO</name><cre_id>PL/658</cre_id>\
            <location><x>-116.9214</x><y>32.47641</y></location></place>\
            <place place_id=\"7\"><name>SIN PRECIO</name><location><x>-100</x><y>20</y></location></place></places>";
        let prices = "<places><place place_id=\"2039\"><gas_price type=\"regular\">22.95</gas_price>\
            <gas_price type=\"premium\">27.9</gas_price></place></places>";
        let out = parse(
            &test_source(Format::Mexico, [14.0, -118.5, 33.0, -86.5]),
            places,
            prices,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].lat, out[0].lon), (32.47641, -116.9214));
        assert_eq!(
            out[0].prices.iter().map(|p| p.fuel).collect::<Vec<_>>(),
            [Fuel::Regular, Fuel::Premium]
        );
        assert_eq!(out[0].prices[0].currency, "MXN");
    }
}
