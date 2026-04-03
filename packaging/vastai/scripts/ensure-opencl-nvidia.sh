#!/usr/bin/env bash
set -euo pipefail

mkdir -p /etc/OpenCL/vendors

export OCL_ICD_VENDORS="${OCL_ICD_VENDORS:-/etc/OpenCL/vendors}"
export OPENCL_VENDOR_PATH="${OPENCL_VENDOR_PATH:-/etc/OpenCL/vendors}"
export LD_LIBRARY_PATH="/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"

detect_nvidia_opencl_library() {
  ldconfig -p 2>/dev/null | awk '/libnvidia-opencl\.so\.1/ { print $NF; exit }'
}

NVIDIA_OPENCL_LIB="$(detect_nvidia_opencl_library || true)"

if [ -z "${NVIDIA_OPENCL_LIB}" ]; then
  NVIDIA_OPENCL_LIB="$(find /usr /usr/local -name 'libnvidia-opencl.so.1' 2>/dev/null | head -n 1 || true)"
fi

if [ -n "${NVIDIA_OPENCL_LIB}" ]; then
  printf '%s\n' "libnvidia-opencl.so.1" >/etc/OpenCL/vendors/nvidia.icd
  cat >/etc/profile.d/blockmine-opencl.sh <<'EOF'
export OCL_ICD_VENDORS=/etc/OpenCL/vendors
export OPENCL_VENDOR_PATH=/etc/OpenCL/vendors
export LD_LIBRARY_PATH=/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}
EOF
  chmod +x /etc/profile.d/blockmine-opencl.sh
  ldconfig 2>/dev/null || true
  echo "[blockmine] configured NVIDIA OpenCL ICD: libnvidia-opencl.so.1 (${NVIDIA_OPENCL_LIB})"
  if command -v clinfo >/dev/null 2>&1; then
    clinfo -l 2>/dev/null || true
  fi
else
  echo "[blockmine] NVIDIA OpenCL library not found yet; GPU mining will wait for a usable OpenCL platform."
fi
