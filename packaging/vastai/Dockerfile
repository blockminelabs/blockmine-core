FROM nvidia/cuda:12.8.0-devel-ubuntu22.04 AS build

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH=/root/.cargo/bin:${PATH}
ENV RUSTFLAGS="--remap-path-prefix=/src=/src/blockmine-core --remap-path-prefix=/root=/usr/home"

RUN apt-get update && apt-get install -y --no-install-recommends \
    bash \
    build-essential \
    ca-certificates \
    clang \
    cmake \
    curl \
    git \
    libssl-dev \
    ocl-icd-opencl-dev \
    opencl-headers \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal

WORKDIR /src

COPY onchain ./onchain
COPY miner-client ./miner-client
COPY packaging/vastai ./packaging/vastai

RUN cargo build --release \
    --manifest-path miner-client/Cargo.toml \
    --features opencl \
    --bin blockmine-miner \
    --bin blockmine-wallet \
    --bin blockmine-vast-worker \
    --bin blockmine-vast-console

FROM nvidia/cuda:12.8.0-runtime-ubuntu22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV BLOCKMINE_STORAGE_DIR=/workspace/blockmine-data
ENV PATH=/usr/local/bin:${PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
    bash \
    ca-certificates \
    clinfo \
    curl \
    libssl3 \
    ocl-icd-libopencl1 \
    procps \
    tmux \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/miner-client/target/release/blockmine-miner /usr/local/bin/blockmine-miner
COPY --from=build /src/miner-client/target/release/blockmine-wallet /usr/local/bin/blockmine-wallet
COPY --from=build /src/miner-client/target/release/blockmine-vast-worker /usr/local/bin/blockmine-vast-worker
COPY --from=build /src/miner-client/target/release/blockmine-vast-console /usr/local/bin/blockmine-vast-console
COPY packaging/vastai/scripts /opt/blockmine/scripts

RUN chmod +x /opt/blockmine/scripts/*.sh

WORKDIR /workspace

CMD ["/bin/bash"]
