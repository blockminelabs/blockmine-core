#!/usr/bin/env bash
set -euo pipefail

mkdir -p /etc/OpenCL/vendors

detect_nvidia_opencl_library() {
  ldconfig -p 2>/dev/null | awk '/libnvidia-opencl\.so\.1/ { print $NF; exit }'
}

NVIDIA_OPENCL_LIB="$(detect_nvidia_opencl_library || true)"

if [ -z "${NVIDIA_OPENCL_LIB}" ]; then
  NVIDIA_OPENCL_LIB="$(find /usr /usr/local -name 'libnvidia-opencl.so.1' 2>/dev/null | head -n 1 || true)"
fi

if [ -n "${NVIDIA_OPENCL_LIB}" ]; then
  printf '%s\n' "${NVIDIA_OPENCL_LIB}" >/etc/OpenCL/vendors/nvidia.icd
  echo "[blockmine] configured NVIDIA OpenCL ICD: ${NVIDIA_OPENCL_LIB}"
else
  echo "[blockmine] NVIDIA OpenCL library not found yet; GPU mining will wait for a usable OpenCL platform."
fi
