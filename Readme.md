# osm-pbf2json

## Build

### Local

```
cargo build --release
```

### Docker

To build linux images on MacOS. The build artifact is located in `./docker-target/release/osm-pbf2json` afterwards.

```
mkdir docker-target
docker run \
  -v $PWD/Cargo.toml:/build/Cargo.toml \
  -v $PWD/Cargo.lock:/build/Cargo.lock \
  -v $PWD/src:/build/src \
  -v $PWD/docker-target:/build/target \
  -w /build \
  rust:1.43 cargo build --release
```

## Run

```
./target/release/osm-pbf2json berlin.pbf > berlin.json
```
