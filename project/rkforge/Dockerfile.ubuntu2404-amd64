# CUDA 13.2.1 devel — Ubuntu 24.04 (amd64)
# Merged from dist/13.2.1/ubuntu2404/{base,runtime,devel}/Dockerfile
# Build: docker build -f Dockerfile.ubuntu2404-amd64 -t cuda:13.2.1-devel-ubuntu24.04 .

FROM ubuntu:24.04

# ── base layer ─────────────────────────────────────────────────────────────────
ENV NVARCH=x86_64
ENV NVIDIA_REQUIRE_CUDA="cuda>=13.2 brand=unknown,driver>=535,driver<536 brand=grid,driver>=535,driver<536 brand=tesla,driver>=535,driver<536 brand=nvidia,driver>=535,driver<536 brand=quadro,driver>=535,driver<536 brand=quadrortx,driver>=535,driver<536 brand=nvidiartx,driver>=535,driver<536 brand=vapps,driver>=535,driver<536 brand=vpc,driver>=535,driver<536 brand=vcs,driver>=535,driver<536 brand=vws,driver>=535,driver<536 brand=cloudgaming,driver>=535,driver<536 brand=unknown,driver>=570,driver<571 brand=grid,driver>=570,driver<571 brand=tesla,driver>=570,driver<571 brand=nvidia,driver>=570,driver<571 brand=quadro,driver>=570,driver<571 brand=quadrortx,driver>=570,driver<571 brand=nvidiartx,driver>=570,driver<571 brand=vapps,driver>=570,driver<571 brand=vpc,driver>=570,driver<571 brand=vcs,driver>=570,driver<571 brand=vws,driver>=570,driver<571 brand=cloudgaming,driver>=570,driver<571 brand=unknown,driver>=580,driver<581 brand=grid,driver>=580,driver<581 brand=tesla,driver>=580,driver<581 brand=nvidia,driver>=580,driver<581 brand=quadro,driver>=580,driver<581 brand=quadrortx,driver>=580,driver<581 brand=nvidiartx,driver>=580,driver<581 brand=vapps,driver>=580,driver<581 brand=vpc,driver>=580,driver<581 brand=vcs,driver>=580,driver<581 brand=vws,driver>=580,driver<581 brand=cloudgaming,driver>=580,driver<581 brand=unknown,driver>=590,driver<591 brand=grid,driver>=590,driver<591 brand=tesla,driver>=590,driver<591 brand=nvidia,driver>=590,driver<591 brand=quadro,driver>=590,driver<591 brand=quadrortx,driver>=590,driver<591 brand=nvidiartx,driver>=590,driver<591 brand=vapps,driver>=590,driver<591 brand=vpc,driver>=590,driver<591 brand=vcs,driver>=590,driver<591 brand=vws,driver>=590,driver<591 brand=cloudgaming,driver>=590,driver<591"
ENV NV_CUDA_CUDART_VERSION=13.2.75-1

LABEL maintainer="NVIDIA CORPORATION <cudatools@nvidia.com>"

RUN apt-get update && apt-get install -y --no-install-recommends \
    gnupg2 curl ca-certificates && \
    curl -fsSLO https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2404/${NVARCH}/cuda-keyring_1.1-1_all.deb && \
    dpkg -i cuda-keyring_1.1-1_all.deb && \
    rm cuda-keyring_1.1-1_all.deb && \
    apt-get purge --autoremove -y curl \
    && rm -rf /var/lib/apt/lists/*

ENV CUDA_VERSION=13.2.1

RUN apt-get update && apt-get install -y --no-install-recommends \
    cuda-cudart-13-2=${NV_CUDA_CUDART_VERSION} \
    cuda-toolkit-13-2-config-common=${NV_CUDA_CUDART_VERSION} \
    cuda-toolkit-13-config-common=${NV_CUDA_CUDART_VERSION} \
    cuda-toolkit-config-common=${NV_CUDA_CUDART_VERSION} \
    && rm -rf /var/lib/apt/lists/*

RUN apt-get update && \
    if apt-cache policy cuda-compat-13-2 2>/dev/null | grep -q "Candidate:"; then \
        apt-get install -y --no-install-recommends cuda-compat-13-2; \
    fi && \
    rm -rf /var/lib/apt/lists/*

RUN apt-get update && \
    if apt-cache policy cuda-compat-orin-13-2 2>/dev/null | grep -q "Candidate:"; then \
        apt-get install -y --no-install-recommends cuda-compat-orin-13-2; \
    fi && \
    rm -rf /var/lib/apt/lists/*

RUN echo "/usr/local/cuda/lib64" >> /etc/ld.so.conf.d/nvidia.conf

ENV PATH=/usr/local/nvidia/bin:/usr/local/cuda/bin:${PATH}
ENV LD_LIBRARY_PATH=/usr/local/nvidia/lib:/usr/local/nvidia/lib64:/usr/local/cuda/lib64

COPY NGC-DL-CONTAINER-LICENSE /

ENV NVIDIA_VISIBLE_DEVICES=all
ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility

# ── runtime layer ──────────────────────────────────────────────────────────────
ENV NV_CUDA_LIB_VERSION=13.2.1-1
ENV NV_NVTX_VERSION=13.2.75-1
ENV NV_LIBNPP_VERSION=13.1.0.48-1
ENV NV_LIBNPP_PACKAGE=libnpp-13-2=${NV_LIBNPP_VERSION}
ENV NV_LIBCUSPARSE_VERSION=12.7.10.1-1
ENV NV_LIBCUBLAS_PACKAGE_NAME=libcublas-13-2
ENV NV_LIBCUBLAS_VERSION=13.4.0.1-1
ENV NV_LIBCUBLAS_PACKAGE=${NV_LIBCUBLAS_PACKAGE_NAME}=${NV_LIBCUBLAS_VERSION}

RUN apt-get update && apt-get install -y --no-install-recommends \
    cuda-libraries-13-2=${NV_CUDA_LIB_VERSION} \
    libnpp-13-2=${NV_LIBNPP_VERSION} \
    cuda-nvtx-13-2=${NV_NVTX_VERSION} \
    libcusparse-13-2=${NV_LIBCUSPARSE_VERSION} \
    libcublas-13-2=${NV_LIBCUBLAS_VERSION} \
    && rm -rf /var/lib/apt/lists/*

RUN apt-mark hold ${NV_LIBCUBLAS_PACKAGE_NAME}

COPY entrypoint.d/ /opt/nvidia/entrypoint.d/
COPY nvidia_entrypoint.sh /opt/nvidia/
ENV NVIDIA_PRODUCT_NAME="CUDA"

# ── devel layer ────────────────────────────────────────────────────────────────
ENV NV_CUDA_CUDART_DEV_VERSION=13.2.75-1
ENV NV_NVML_DEV_VERSION=13.2.82-1
ENV NV_LIBCUSPARSE_DEV_VERSION=12.7.10.1-1
ENV NV_LIBNPP_DEV_VERSION=13.1.0.48-1
ENV NV_LIBCUBLAS_DEV_VERSION=13.4.0.1-1
ENV NV_LIBCUBLAS_DEV_PACKAGE_NAME=libcublas-dev-13-2
ENV NV_CUDA_NSIGHT_COMPUTE_VERSION=13.2.1-1

RUN apt-get update && apt-get install -y --no-install-recommends \
    cuda-cudart-dev-13-2=${NV_CUDA_CUDART_DEV_VERSION} \
    cuda-command-line-tools-13-2=${NV_CUDA_LIB_VERSION} \
    cuda-minimal-build-13-2=${NV_CUDA_LIB_VERSION} \
    cuda-libraries-dev-13-2=${NV_CUDA_LIB_VERSION} \
    cuda-nvml-dev-13-2=${NV_NVML_DEV_VERSION} \
    libnpp-dev-13-2=${NV_LIBNPP_DEV_VERSION} \
    libcusparse-dev-13-2=${NV_LIBCUSPARSE_DEV_VERSION} \
    libcublas-dev-13-2=${NV_LIBCUBLAS_DEV_VERSION} \
    cuda-nsight-compute-13-2=${NV_CUDA_NSIGHT_COMPUTE_VERSION} \
    && rm -rf /var/lib/apt/lists/*

RUN NSYS_PATH=$(find /opt/nvidia/nsight-compute -type f -name "nsys" 2>/dev/null | head -n 1); \
    if [ -n "$NSYS_PATH" ] && [ ! -f /usr/local/bin/nsys ]; then \
        ln -s "$NSYS_PATH" /usr/local/bin/nsys; \
    fi

RUN apt-mark hold ${NV_LIBCUBLAS_DEV_PACKAGE_NAME}

ENV LIBRARY_PATH=/usr/local/cuda/lib64/stubs

ENTRYPOINT ["/opt/nvidia/nvidia_entrypoint.sh"]
