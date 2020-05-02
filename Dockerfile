FROM rust:1.43

RUN mkdir /build
COPY ./src /build/src
COPY ./Cargo.lock /build
COPY ./Cargo.toml /build
WORKDIR /build
