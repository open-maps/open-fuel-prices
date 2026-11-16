mod http;
mod model;
mod publish;
mod sources;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{env, fs, thread};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use log::{error, info};
use serde::Deserialize;

use http::Http;
use publish::SourceState;
use sources::Source;

#[derive(Deserialize)]
struct Config {
    user_agent: String,
    out_dir: PathBuf,
    state_dir: PathBuf,
    /// Prices reported longer ago than this are left out.
    #[serde(default = "default_max_price_age_days")]
    max_price_age_days: i64,
    #[serde(rename = "source")]
    sources: Vec<Source>,
}

fn default_max_price_age_days() -> i64 {
    30
}

const USAGE: &str = "usage: open-fuel-prices <run|once|check SOURCE_ID> (config from $OFP_CONFIG or ./config.toml)";

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let path = env::var("OFP_CONFIG").unwrap_or_else(|_| "config.toml".into());
    let config: Config = toml::from_str(&fs::read_to_string(&path).with_context(|| format!("reading {path}"))?)?;
    let args: Vec<String> = env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["run"] => run(&config, false),
        ["once"] => run(&config, true),
        ["check", id] => check(&config, id),
        _ => bail!(USAGE),
    }
}

fn state_path(config: &Config, id: &str) -> PathBuf {
    config.state_dir.join(format!("{id}.json"))
}

fn run(config: &Config, once: bool) -> Result<()> {
    let http = Http::new(&config.user_agent)?;
    let enabled: Vec<&Source> = config.sources.iter().filter(|s| s.enabled).collect();
    let mut states: BTreeMap<String, SourceState> = enabled
        .iter()
        .filter_map(|src| {
            let bytes = fs::read(state_path(config, &src.id)).ok()?;
            Some((src.id.clone(), serde_json::from_slice(&bytes).ok()?))
        })
        .collect();
    let mut attempts: BTreeMap<String, Instant> = BTreeMap::new();
    let mut changed = !states.is_empty();
    loop {
        for src in &enabled {
            if attempts
                .get(&src.id)
                .is_some_and(|t| t.elapsed() < Duration::from_secs(src.poll_secs))
            {
                continue;
            }
            attempts.insert(src.id.clone(), Instant::now());
            match sources::fetch(src, &http) {
                Ok(fetched) => {
                    info!(
                        "{}: {} stations, {} with swapped coordinates, {} outside the source's area",
                        src.id,
                        fetched.stations.len(),
                        fetched.swapped,
                        fetched.dropped
                    );
                    let state = SourceState {
                        fetched: Utc::now(),
                        stations: fetched.stations,
                    };
                    publish::write_atomic(&state_path(config, &src.id), &serde_json::to_vec(&state)?)?;
                    states.insert(src.id.clone(), state);
                    changed = true;
                }
                Err(e) => error!("{}: {e:#}", src.id),
            }
        }
        if changed {
            let max_age = chrono::Duration::days(config.max_price_age_days);
            match publish::publish(&config.out_dir, &enabled, &states, Utc::now(), max_age) {
                Ok((stations, cells)) => info!("published {stations} stations in {cells} cells"),
                Err(e) => error!("publishing: {e:#}"),
            }
            changed = false;
        }
        if once {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(30));
    }
}

fn check(config: &Config, id: &str) -> Result<()> {
    let src = config
        .sources
        .iter()
        .find(|s| s.id == id)
        .with_context(|| format!("no source {id}"))?;
    let fetched = sources::fetch(src, &Http::new(&config.user_agent)?)?;
    let cutoff = Utc::now() - chrono::Duration::days(config.max_price_age_days);
    let mut fuels: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for p in fetched.stations.iter().flat_map(|s| &s.prices) {
        let entry = fuels.entry(format!("{:?}", p.fuel)).or_default();
        entry.0 += 1;
        if p.updated.is_some_and(|t| t < cutoff) {
            entry.1 += 1;
        }
    }
    let times: Vec<_> = fetched
        .stations
        .iter()
        .flat_map(|s| &s.prices)
        .filter_map(|p| p.updated)
        .collect();
    println!(
        "{id}: {} stations, {} swapped, {} outside the area",
        fetched.stations.len(),
        fetched.swapped,
        fetched.dropped
    );
    for (fuel, (n, old)) in fuels {
        println!(
            "  {fuel}: {n} prices, {old} older than {} days",
            config.max_price_age_days
        );
    }
    if let (Some(min), Some(max)) = (times.iter().min(), times.iter().max()) {
        println!("  reported between {min} and {max}");
    }
    if let Some(s) = fetched.stations.first() {
        println!("  first: {}", serde_json::to_string(s)?);
    }
    Ok(())
}
