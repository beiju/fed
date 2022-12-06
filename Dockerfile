FROM rustlang/rust:nightly as builder

WORKDIR /usr/src/fed
COPY . .

RUN cargo build --bin fed_server --release

FROM debian:buster-slim

RUN apt-get update && \
    apt-get dist-upgrade -y&& \
    apt-get install libssl-dev ca-certificates -y

COPY --from=builder /usr/src/fed/target/release/fed_server .

USER 1000
CMD ["./fed_server"]
