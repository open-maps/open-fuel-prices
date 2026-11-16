use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fuel {
    Diesel,
    #[serde(rename = "petrol_95")]
    Petrol95,
    E10,
    #[serde(rename = "petrol_98")]
    Petrol98,
    E85,
    Lpg,
    /// Priced per kilogram.
    Cng,
    /// The basic grade where it is not 95 octane, such as Mexico's 87.
    Regular,
    /// The better grade where it is not 98 octane.
    Premium,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub fuel: Fuel,
    pub price: f64,
    /// ISO 4217 code.
    pub currency: String,
    /// When the station reported the price. Null when the source does not say.
    pub updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Station {
    /// Unique across sources, prefixed with the source id.
    pub id: String,
    pub source: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub prices: Vec<Price>,
}

/// A local time in `format`, read in zone `tz`.
pub fn local_time(value: &str, format: &str, tz: Tz) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(value.trim(), format).ok()?;
    tz.from_local_datetime(&naive).earliest().map(|t| t.to_utc())
}

pub fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn non_blank(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// "street, postcode town", leaving out the parts a source does not give.
pub fn address(street: Option<String>, postcode: Option<String>, town: Option<String>) -> Option<String> {
    let place = [postcode, town].into_iter().flatten().collect::<Vec<_>>().join(" ");
    let parts: Vec<String> = [street, Some(place)]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_local_times_in_their_zone() {
        let t = local_time("16/11/2026 22:57:44", "%d/%m/%Y %H:%M:%S", chrono_tz::Europe::Madrid).unwrap();
        assert_eq!(t.to_rfc3339(), "2026-11-16T21:57:44+00:00");
    }

    #[test]
    fn formats_names_and_addresses() {
        assert_eq!(title_case("AVENIDA CASTILLA  LA MANCHA"), "Avenida Castilla La Mancha");
        assert_eq!(
            address(Some("Rue X".into()), Some("56580".into()), Some("Bréhan".into())).as_deref(),
            Some("Rue X, 56580 Bréhan")
        );
        assert_eq!(address(None, None, None), None);
    }
}
