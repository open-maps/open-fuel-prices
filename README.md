# open-fuel-prices

Collects per-station fuel prices from countries that publish them as whole-country files and serves them as small static JSON files, cut into grid cells, so apps fetch only the area they show.

| Source | Country | Refresh | Licence |
|---|---|---|---|
| prix-carburants.gouv.fr instant feed | France | 10 min | Licence Ouverte 2.0 |
| Ministerio para la Transición Ecológica | Spain | 30 min | reuse with credit |
| DGEG | mainland Portugal | 30 min | reuse with credit and update time |
| MIMIT | Italy | daily | IODL 2.0 |
| CNE | Mexico | hourly, no report times | free use, "Fuente: CNE" |

Prices reported more than `max_price_age_days` ago are left out.

## Output

`v1/sources.json` lists the sources with their attribution, licence, area and last fetch, and gives the cell size and path pattern. `v1/cells/{x}/{y}.json` holds the stations of one cell as `{"stations": [...]}`, where

```
x = floor((lon + 180) / cell_degrees)
y = floor((lat + 90) / cell_degrees)
```

A missing cell has no stations. Each station has an `id`, `source`, `lat`, `lon`, optional `name`, `brand` and `address`, and `prices` with `fuel`, `price`, `currency` and `updated` (null when the source gives no time). Fuels are `diesel`, `petrol_95`, `e10`, `petrol_98`, `e85`, `lpg`, `cng` (per kg), and `regular` and `premium` for grades that are not 95 and 98 octane.

## Running

```
nix develop
cargo run --release -- check it   # fetch one source and summarise it
cargo run --release -- once       # fetch everything and publish to ./out
cargo run --release -- run        # keep polling
```

The NixOS module in `flake.nix` runs the collector as `services.open-fuel-prices` and serves `out` under `path` on an nginx virtual host, which other services may share.

## Licence

AGPL-3.0-or-later.
