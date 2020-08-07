# osm-pbf2json

[![Build Status](https://travis-ci.org/mkulke/osm-pbf2json.svg?branch=master)](https://travis-ci.org/mkulke/osm-pbf2json)
[![codecov](https://codecov.io/gh/mkulke/osm-pbf2json/branch/master/graph/badge.svg)](https://codecov.io/gh/mkulke/osm-pbf2json)
[![crates.io](https://img.shields.io/crates/v/osm_pbf2json.svg)](https://crates.io/crates/osm_pbf2json)

A parser/filter for OSM protobuf bundles

## Build

### Local

A rust build environment can be installed via [rustup](https://rustup.rs/).

```
cargo test
cargo build --release
```

### Docker

To build linux images on MacOS. The build artifact is located in `./docker-target/release` afterwards.

```
mkdir docker-target
docker run \
  -v $PWD/Cargo.toml:/build/Cargo.toml \
  -v $PWD/Cargo.lock:/build/Cargo.lock \
  -v $PWD/src:/build/src \
  -v $PWD/benches:/build/benches \
  -v $PWD/tests:/build/tests \
  -v $PWD/docker-target:/build/target \
  -w /build \
  rust:1.43 cargo build --release
```

## Run

### Retrieve Objects

You have to specify a query via `--tags` or `-t`, the syntax is rather simple:

By stating a key (`-t amenity`) it will select all entities which are tagged using that key. To further narrow down the results, a specific value can be given using a `~` field separator (`-t 'amenity~fountain'`). To check the presence of multiple tags for the same entity, statements can be combined using the `+` operator (`-t 'amenity~fountain+tourism'`). Finally, options can be specified by concatenating groups of statements with `,` (`-t 'amenity~fountain+tourism,amenity~townhall'`). If an entity matches the criteria of either group it will be included in the output.

A clipped PBF sample is contained in the `./tests/data` folder.

```
./target/release/osm_pbf2json berlin.pbf objects -t="addr:housenumber+addr:street+addr:postcode~10178" | tail -3
{"id":544604702,"type":"way","tags":{"addr:city":"Berlin","addr:country":"DE","addr:housenumber":"17","addr:postcode":"10178","addr:street":"Sophienstraße","addr:suburb":"Mitte","building":"residential","heritage":"4","heritage:operator":"lda","lda:criteria":"Ensembleteil","ref:lda":"09080182"},"centroid":{"lat":52.52571770265661,"lon":13.401513737828404},"bounds":{"e":13.4015869,"n":52.525649699999995,"s":52.5254975,"w":13.4013709}}
{"id":569067822,"type":"way","tags":{"addr:city":"Berlin","addr:housenumber":"1","addr:postcode":"10178","addr:street":"Anna-Louisa-Karsch-Straße","amenity":"library","email":"theol@ub.hu-berlin.de","internet_access":"wlan","internet_access:fee":"no","name":"Humboldt-Universität zu Berlin Universitätsbibliothek Zweigbibliothek Theologie","name:en":"Humboldt University of Berlin University Library Theology Branch Library","opening_hours":"Mo-Fr 09:30-20:30; Sa 09:30-13:30","operator":"Universitätsbibliothek der Humboldt-Universität zu Berlin","phone":"+49 30 2093-91800","ref:isil":"DE-11-133","website":"https://www.ub.hu-berlin.de/de/standorte/zwbtheologie","wheelchair":"yes","wikidata":"Q73146656"},"centroid":{"lat":52.52113243392209,"lon":13.401373781610301},"bounds":{"e":13.4016009,"n":52.521282,"s":52.520961,"w":13.4011406}}
{"id":625034881,"type":"way","tags":{"addr:city":"Berlin","addr:housenumber":"1","addr:postcode":"10178","addr:street":"Karl-Marx-Allee","building":"commercial","building:levels":"1","height":"5","name":"Werkstatt Haus der Statistik"},"centroid":{"lat":52.52212174147001,"lon":13.418398630534096},"bounds":{"e":13.418516499999999,"n":52.5221701,"s":52.5220175,"w":13.4182626}}
```

### Extract Streets

Streets are represented in OSM as a collection of smaller road segments. To group those segments into street entities a few heuristics are employed, specifically the name tag and the geographical distance. A boundary level can be specified to split a street along boundary lines.

```
./target/release/osm_pbf2json tests/data/alexanderplatz.pbf streets --geojson -n="Gontardstraße" 
{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"name":"Gontardstraße","stroke":"#7DA86A"},"geometry":{"type":"MultiLineString","coordinates":[[[13.410188699999999,52.521660999999995],[13.4108953,52.521203799999995],[13.410997,52.521133199999994],[13.4114945,52.5208095],[13.4119613,52.520479099999996]],[[13.410188699999999,52.521660999999995],[13.410212399999999,52.521679899999995],[13.4102321,52.5216956],[13.4102623,52.5217192],[13.4102997,52.5217484]],[[13.4095035,52.522308699999996],[13.4095806,52.5222255],[13.4096047,52.5221899],[13.4098305,52.5220348],[13.4102997,52.5217484]]]}}]}
```

### Extract Administrative Boundaries

Admin Boundaries are stored as OSM Relations (e.g. Country, State) with complex and disconnected geometry, if required. The levels of a boundary are specific per country, a list can be found [here](https://wiki.openstreetmap.org/wiki/Tag:boundary%3Dadministrative#10_admin_level_values_for_specific_countries). Several boundary levels can be specified and extracted in a single run. By default levels 4, 6, 8, 9 & 10 are considered. GeoJSON output is available for this option.

```
./target/release/osm_pbf2json berlin.pbf boundaries -l 9
{"name":"Reinickendorf","admin_level":9,"bbox":{"sw":[13.201617599999999,52.5488072],"ne":[13.3892817,52.660741099999996]}}
{"name":"Spandau","admin_level":9,"bbox":{"sw":[13.109295,52.439614999999996],"ne":[13.2824665,52.598796899999996]}}
{"name":"Mitte","admin_level":9,"bbox":{"sw":[13.3015376,52.4987357],"ne":[13.4294017,52.5676686]}}
{"name":"Steglitz-Zehlendorf","admin_level":9,"bbox":{"sw":[13.088344999999999,52.3872254],"ne":[13.3716004,52.4718369]}}
{"name":"Treptow-Köpenick","admin_level":9,"bbox":{"sw":[13.4396363,52.3382448],"ne":[13.7611609,52.497706799999996]}}
{"name":"Friedrichshain-Kreuzberg","admin_level":9,"bbox":{"sw":[13.368229099999999,52.4827923],"ne":[13.4914434,52.5310256]}}
{"name":"Gosen","admin_level":9,"bbox":{"sw":[13.6861251,52.377441399999995],"ne":[13.7228219,52.399721299999996]}}
{"name":"Tempelhof-Schöneberg","admin_level":9,"bbox":{"sw":[13.3199923,52.376138399999995],"ne":[13.42746,52.5049424]}}
{"name":"Neukölln","admin_level":9,"bbox":{"sw":[13.3994933,52.395945399999995],"ne":[13.5241327,52.495864999999995]}}
{"name":"Marzahn-Hellersdorf","admin_level":9,"bbox":{"sw":[13.5168837,52.4704779],"ne":[13.658503399999999,52.574508599999994]}}
{"name":"Pankow","admin_level":9,"bbox":{"sw":[13.3475571,52.519927599999995],"ne":[13.523022,52.675508699999995]}}
{"name":"Charlottenburg-Wilmersdorf","admin_level":9,"bbox":{"sw":[13.1865954,52.4664729],"ne":[13.3414287,52.5494336]}}
{"name":"Lichtenberg","admin_level":9,"bbox":{"sw":[13.456196499999999,52.4678355],"ne":[13.5677059,52.5964629]}}
{"name":"Lindenberg","admin_level":9,"bbox":{"sw":[13.4966616,52.586616899999996],"ne":[13.566374999999999,52.620224099999994]}}
{"name":"Schönerlinde","admin_level":9,"bbox":{"sw":[13.3979077,52.6354682],"ne":[13.4742692,52.6734271]}}
```

## Test

```
cargo test
cargo bench
```
