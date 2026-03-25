#[cfg(feature = "opencl")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "opencl")]
use std::time::{Duration, Instant};

#[cfg(not(feature = "opencl"))]
use anyhow::anyhow;
#[cfg(feature = "opencl")]
use anyhow::Context;
use anyhow::Result;

use crate::engine::{BackendKind, BenchmarkReport, FoundSolution, MiningEngine, SearchInput};

#[derive(Clone)]
pub struct GpuMiner {
    platform_index: usize,
    device_index: usize,
    local_work_size: Option<usize>,
    #[cfg(feature = "opencl")]
    runtime: Arc<Mutex<Option<GpuRuntime>>>,
}

impl std::fmt::Debug for GpuMiner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuMiner")
            .field("platform_index", &self.platform_index)
            .field("device_index", &self.device_index)
            .field("local_work_size", &self.local_work_size)
            .finish()
    }
}

impl GpuMiner {
    pub fn new(platform_index: usize, device_index: usize, local_work_size: Option<usize>) -> Self {
        Self {
            platform_index,
            device_index,
            local_work_size,
            #[cfg(feature = "opencl")]
            runtime: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(feature = "opencl")]
    pub fn benchmark_with_batch_size(
        &self,
        seconds: u64,
        batch_size: u64,
    ) -> Result<BenchmarkReport> {
        run_opencl_benchmark(
            self,
            self.platform_index,
            self.device_index,
            self.local_work_size,
            seconds,
            batch_size,
        )
    }

    #[cfg(not(feature = "opencl"))]
    pub fn benchmark_with_batch_size(
        &self,
        _seconds: u64,
        _batch_size: u64,
    ) -> Result<BenchmarkReport> {
        Err(anyhow!(
            "GPU benchmark requires building the miner with `--features opencl` and an installed OpenCL runtime."
        ))
    }
}

#[cfg(feature = "opencl")]
struct GpuRuntime {
    pro_que: ocl::ProQue,
    capacity: usize,
    local_work_size: Option<usize>,
    challenge_buffer: ocl::Buffer<u8>,
    miner_buffer: ocl::Buffer<u8>,
    target_buffer: ocl::Buffer<u8>,
    found_flag_buffer: ocl::Buffer<u32>,
    found_nonce_buffer: ocl::Buffer<u64>,
    found_hash_buffer: ocl::Buffer<u8>,
    last_challenge: [u8; 32],
    last_miner: [u8; 32],
    last_target: [u8; 32],
    input_bound: bool,
}

#[cfg(feature = "opencl")]
impl GpuRuntime {
    fn bind_static_inputs(
        &mut self,
        challenge: &[u8; 32],
        miner: &[u8; 32],
        target: &[u8; 32],
    ) -> Result<()> {
        if !self.input_bound || self.last_challenge != *challenge {
            self.challenge_buffer
                .write(&challenge[..])
                .enq()
                .context("failed to update OpenCL challenge buffer")?;
            self.last_challenge = *challenge;
        }

        if !self.input_bound || self.last_miner != *miner {
            self.miner_buffer
                .write(&miner[..])
                .enq()
                .context("failed to update OpenCL miner buffer")?;
            self.last_miner = *miner;
        }

        if !self.input_bound || self.last_target != *target {
            self.target_buffer
                .write(&target[..])
                .enq()
                .context("failed to update OpenCL target buffer")?;
            self.last_target = *target;
        }

        self.input_bound = true;
        Ok(())
    }

    fn reset_found_state(&self) -> Result<()> {
        let found_flag = [0u32; 1];
        let found_nonce = [0u64; 1];
        let found_hash = [0u8; 32];

        self.found_flag_buffer
            .write(&found_flag[..])
            .enq()
            .context("failed to reset GPU found flag")?;
        self.found_nonce_buffer
            .write(&found_nonce[..])
            .enq()
            .context("failed to reset GPU found nonce")?;
        self.found_hash_buffer
            .write(&found_hash[..])
            .enq()
            .context("failed to reset GPU found hash")?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct GpuDeviceInfo {
    pub platform_index: usize,
    pub device_index: usize,
    pub platform_name: String,
    pub device_name: String,
    pub vendor: String,
    pub version: String,
    pub global_memory_bytes: u64,
    pub max_compute_units: u32,
    pub max_work_group_size: usize,
}

#[cfg(not(feature = "opencl"))]
pub fn list_devices() -> Result<Vec<GpuDeviceInfo>> {
    Err(anyhow!(
        "Listing GPU devices requires building the miner with `--features opencl` and an installed OpenCL runtime."
    ))
}

#[cfg(feature = "opencl")]
pub fn list_devices() -> Result<Vec<GpuDeviceInfo>> {
    use ocl::enums::{DeviceInfo, DeviceInfoResult};
    use ocl::Platform;

    let mut items = Vec::new();
    for (platform_index, platform) in Platform::list().into_iter().enumerate() {
        let platform_name = platform
            .name()
            .context("failed to read OpenCL platform name")?;
        let devices = ocl::Device::list_all(platform).with_context(|| {
            format!(
                "failed to enumerate OpenCL devices for platform {}",
                platform_index
            )
        })?;

        for (device_index, device) in devices.into_iter().enumerate() {
            let device_name = device.name().context("failed to read OpenCL device name")?;
            let vendor = device
                .vendor()
                .context("failed to read OpenCL device vendor")?;
            let version = device
                .version()
                .context("failed to read OpenCL device version")?
                .to_string();
            let global_memory_bytes = match device
                .info(DeviceInfo::GlobalMemSize)
                .context("failed to read OpenCL global memory size")?
            {
                DeviceInfoResult::GlobalMemSize(bytes) => bytes,
                _ => 0,
            };
            let max_compute_units = match device
                .info(DeviceInfo::MaxComputeUnits)
                .context("failed to read OpenCL compute units")?
            {
                DeviceInfoResult::MaxComputeUnits(units) => units,
                _ => 0,
            };
            let max_work_group_size = match device
                .info(DeviceInfo::MaxWorkGroupSize)
                .context("failed to read OpenCL work group size")?
            {
                DeviceInfoResult::MaxWorkGroupSize(size) => size,
                _ => 0,
            };

            items.push(GpuDeviceInfo {
                platform_index,
                device_index,
                platform_name: platform_name.clone(),
                device_name,
                vendor,
                version,
                global_memory_bytes,
                max_compute_units,
                max_work_group_size,
            });
        }
    }

    Ok(items)
}

#[cfg(not(feature = "opencl"))]
impl MiningEngine for GpuMiner {
    fn kind(&self) -> BackendKind {
        BackendKind::Gpu
    }

    fn search_batch(&self, _input: &SearchInput) -> Result<Option<FoundSolution>> {
        Err(anyhow!(
            "GPU backend requires building the miner with `--features opencl` and an installed OpenCL runtime."
        ))
    }

    fn benchmark(&self, _seconds: u64) -> Result<BenchmarkReport> {
        Err(anyhow!(
            "GPU benchmark requires building the miner with `--features opencl` and an installed OpenCL runtime."
        ))
    }
}

#[cfg(feature = "opencl")]
impl MiningEngine for GpuMiner {
    fn kind(&self) -> BackendKind {
        BackendKind::Gpu
    }

    fn search_batch(&self, input: &SearchInput) -> Result<Option<FoundSolution>> {
        run_opencl_search(
            self,
            self.platform_index,
            self.device_index,
            self.local_work_size,
            input,
        )
    }

    fn benchmark(&self, seconds: u64) -> Result<BenchmarkReport> {
        self.benchmark_with_batch_size(seconds, 4_194_304)
    }
}

#[cfg(feature = "opencl")]
fn run_opencl_search(
    miner: &GpuMiner,
    platform_index: usize,
    device_index: usize,
    local_work_size: Option<usize>,
    input: &SearchInput,
) -> Result<Option<FoundSolution>> {
    use ocl::Kernel;

    let started = Instant::now();
    let miner_bytes = input.miner.to_bytes();
    let mut runtime_guard = miner
        .runtime
        .lock()
        .map_err(|_| anyhow::anyhow!("failed to lock GPU runtime"))?;
    let required_capacity = input.max_attempts as usize;
    let should_rebuild = runtime_guard
        .as_ref()
        .map(|runtime| required_capacity > runtime.capacity)
        .unwrap_or(true);

    if should_rebuild {
        *runtime_guard = Some(build_gpu_runtime(
            platform_index,
            device_index,
            local_work_size,
            required_capacity,
        )?);
    }

    let runtime = runtime_guard
        .as_mut()
        .expect("GPU runtime must exist after initialization");
    runtime.bind_static_inputs(&input.challenge, &miner_bytes, &input.target)?;
    runtime.reset_found_state()?;

    let global_work_size = align_global_work_size(required_capacity, runtime.local_work_size);
    let mut kernel_builder_base = Kernel::builder();
    let mut kernel_builder = kernel_builder_base
        .program(runtime.pro_que.program())
        .name("mine_sha256")
        .queue(runtime.pro_que.queue().clone())
        .global_work_size(global_work_size)
        .arg(&runtime.challenge_buffer)
        .arg(&runtime.miner_buffer)
        .arg(&runtime.target_buffer)
        .arg(input.start_nonce)
        .arg(input.max_attempts)
        .arg(&runtime.found_flag_buffer)
        .arg(&runtime.found_nonce_buffer)
        .arg(&runtime.found_hash_buffer);

    if let Some(local) = runtime.local_work_size {
        kernel_builder = kernel_builder.local_work_size(local);
    }

    let kernel = kernel_builder
        .build()
        .context("failed to build OpenCL kernel")?;

    unsafe {
        kernel
            .enq()
            .context("failed to enqueue OpenCL mining kernel")?;
    }
    runtime
        .pro_que
        .queue()
        .finish()
        .context("failed to finish GPU queue")?;

    let mut found_flag = [0u32; 1];
    let mut found_nonce = [0u64; 1];
    let mut found_hash = [0u8; 32];
    runtime
        .found_flag_buffer
        .read(&mut found_flag[..])
        .enq()
        .context("failed to read found flag")?;
    if found_flag[0] == 0 {
        return Ok(None);
    }

    runtime
        .found_nonce_buffer
        .read(&mut found_nonce[..])
        .enq()
        .context("failed to read found nonce")?;
    runtime
        .found_hash_buffer
        .read(&mut found_hash[..])
        .enq()
        .context("failed to read found hash")?;

    Ok(Some(FoundSolution {
        backend: BackendKind::Gpu,
        nonce: found_nonce[0],
        hash: found_hash,
        attempts: input.max_attempts,
        elapsed: started.elapsed(),
    }))
}

#[cfg(feature = "opencl")]
fn run_opencl_benchmark(
    miner: &GpuMiner,
    platform_index: usize,
    device_index: usize,
    local_work_size: Option<usize>,
    seconds: u64,
    batch_size: u64,
) -> Result<BenchmarkReport> {
    let started = Instant::now();
    let challenge = [7u8; 32];
    let benchmark_miner = solana_sdk::pubkey::Pubkey::new_unique();
    let impossible_target = [0u8; 32];
    let mut start_nonce = 0u64;
    let mut hashes = 0u64;

    while started.elapsed() < Duration::from_secs(seconds) {
        let input = SearchInput {
            challenge,
            miner: benchmark_miner,
            target: impossible_target,
            start_nonce,
            max_attempts: batch_size,
        };
        let _ = run_opencl_search(miner, platform_index, device_index, local_work_size, &input)?;
        hashes = hashes.saturating_add(batch_size);
        start_nonce = start_nonce.wrapping_add(batch_size);
    }

    Ok(BenchmarkReport {
        backend: BackendKind::Gpu,
        hashes,
        elapsed: started.elapsed(),
    })
}

#[cfg(feature = "opencl")]
fn build_gpu_runtime(
    platform_index: usize,
    device_index: usize,
    requested_local_work_size: Option<usize>,
    requested_capacity: usize,
) -> Result<GpuRuntime> {
    use ocl::enums::{DeviceInfo, DeviceInfoResult};
    use ocl::{flags, Buffer, Platform, ProQue};

    let platforms = Platform::list();
    let platform = platforms
        .get(platform_index)
        .cloned()
        .with_context(|| format!("OpenCL platform index {} is not available", platform_index))?;
    let devices = ocl::Device::list_all(platform).context("failed to enumerate OpenCL devices")?;
    let device = devices
        .get(device_index)
        .cloned()
        .with_context(|| format!("OpenCL device index {} is not available", device_index))?;

    let max_work_group_size = match device
        .info(DeviceInfo::MaxWorkGroupSize)
        .context("failed to read OpenCL work group size")?
    {
        DeviceInfoResult::MaxWorkGroupSize(size) => size,
        _ => 0,
    };

    let local_work_size = resolve_local_work_size(
        max_work_group_size,
        requested_local_work_size,
        requested_capacity,
    );
    let capacity = align_global_work_size(requested_capacity.max(1), local_work_size);
    let pro_que = ProQue::builder()
        .platform(platform)
        .device(device)
        .src(SHA256_OPENCL_KERNEL)
        .dims(capacity)
        .build()
        .context("failed to build OpenCL program")?;

    let challenge_buffer = Buffer::<u8>::builder()
        .queue(pro_que.queue().clone())
        .flags(flags::MEM_READ_ONLY)
        .len(32)
        .build()
        .context("failed to create challenge buffer")?;
    let miner_buffer = Buffer::<u8>::builder()
        .queue(pro_que.queue().clone())
        .flags(flags::MEM_READ_ONLY)
        .len(32)
        .build()
        .context("failed to create miner buffer")?;
    let target_buffer = Buffer::<u8>::builder()
        .queue(pro_que.queue().clone())
        .flags(flags::MEM_READ_ONLY)
        .len(32)
        .build()
        .context("failed to create target buffer")?;
    let found_flag_buffer = Buffer::<u32>::builder()
        .queue(pro_que.queue().clone())
        .flags(flags::MEM_READ_WRITE)
        .len(1)
        .build()
        .context("failed to create found flag buffer")?;
    let found_nonce_buffer = Buffer::<u64>::builder()
        .queue(pro_que.queue().clone())
        .flags(flags::MEM_READ_WRITE)
        .len(1)
        .build()
        .context("failed to create found nonce buffer")?;
    let found_hash_buffer = Buffer::<u8>::builder()
        .queue(pro_que.queue().clone())
        .flags(flags::MEM_READ_WRITE)
        .len(32)
        .build()
        .context("failed to create found hash buffer")?;

    Ok(GpuRuntime {
        pro_que,
        capacity,
        local_work_size,
        challenge_buffer,
        miner_buffer,
        target_buffer,
        found_flag_buffer,
        found_nonce_buffer,
        found_hash_buffer,
        last_challenge: [0u8; 32],
        last_miner: [0u8; 32],
        last_target: [0u8; 32],
        input_bound: false,
    })
}

#[cfg(feature = "opencl")]
fn resolve_local_work_size(
    max_work_group_size: usize,
    requested_local_work_size: Option<usize>,
    requested_capacity: usize,
) -> Option<usize> {
    let upper_bound = max_work_group_size.min(requested_capacity.max(1));
    if upper_bound < 2 {
        return None;
    }

    let preferred = requested_local_work_size.unwrap_or_else(|| {
        if upper_bound >= 256 {
            256
        } else if upper_bound >= 128 {
            128
        } else if upper_bound >= 64 {
            64
        } else if upper_bound >= 32 {
            32
        } else {
            upper_bound
        }
    });

    let bounded = preferred.min(upper_bound);
    let tuned = floor_power_of_two(bounded);
    (tuned >= 2).then_some(tuned)
}

#[cfg(feature = "opencl")]
fn floor_power_of_two(value: usize) -> usize {
    if value <= 1 {
        return value;
    }

    1usize << (usize::BITS as usize - 1 - value.leading_zeros() as usize)
}

#[cfg(feature = "opencl")]
fn align_global_work_size(value: usize, local_work_size: Option<usize>) -> usize {
    let value = value.max(1);
    match local_work_size {
        Some(local) if local > 1 => value.div_ceil(local) * local,
        _ => value,
    }
}

#[cfg(feature = "opencl")]
const SHA256_OPENCL_KERNEL: &str = r#"
__constant uint K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
};

uint rotr32(uint x, uint n) { return (x >> n) | (x << (32 - n)); }
uint ch(uint x, uint y, uint z) { return (x & y) ^ (~x & z); }
uint maj(uint x, uint y, uint z) { return (x & y) ^ (x & z) ^ (y & z); }
uint bsig0(uint x) { return rotr32(x, 2) ^ rotr32(x, 13) ^ rotr32(x, 22); }
uint bsig1(uint x) { return rotr32(x, 6) ^ rotr32(x, 11) ^ rotr32(x, 25); }
uint ssig0(uint x) { return rotr32(x, 7) ^ rotr32(x, 18) ^ (x >> 3); }
uint ssig1(uint x) { return rotr32(x, 17) ^ rotr32(x, 19) ^ (x >> 10); }

void sha256_fixed_72(uchar message[72], uchar digest[32]) {
    uchar padded[128];
    for (int i = 0; i < 128; i++) {
        padded[i] = 0;
    }
    for (int i = 0; i < 72; i++) {
        padded[i] = message[i];
    }
    padded[72] = 0x80;
    ulong bit_length = (ulong)72 * 8;
    padded[127] = (uchar)(bit_length & 0xff);
    padded[126] = (uchar)((bit_length >> 8) & 0xff);
    padded[125] = (uchar)((bit_length >> 16) & 0xff);
    padded[124] = (uchar)((bit_length >> 24) & 0xff);
    padded[123] = (uchar)((bit_length >> 32) & 0xff);
    padded[122] = (uchar)((bit_length >> 40) & 0xff);
    padded[121] = (uchar)((bit_length >> 48) & 0xff);
    padded[120] = (uchar)((bit_length >> 56) & 0xff);

    uint h0 = 0x6a09e667;
    uint h1 = 0xbb67ae85;
    uint h2 = 0x3c6ef372;
    uint h3 = 0xa54ff53a;
    uint h4 = 0x510e527f;
    uint h5 = 0x9b05688c;
    uint h6 = 0x1f83d9ab;
    uint h7 = 0x5be0cd19;

    for (int block = 0; block < 2; block++) {
        uint w[64];
        int offset = block * 64;
        for (int i = 0; i < 16; i++) {
            int j = offset + (i * 4);
            w[i] = ((uint)padded[j] << 24) |
                   ((uint)padded[j + 1] << 16) |
                   ((uint)padded[j + 2] << 8) |
                   ((uint)padded[j + 3]);
        }
        for (int i = 16; i < 64; i++) {
            w[i] = ssig1(w[i - 2]) + w[i - 7] + ssig0(w[i - 15]) + w[i - 16];
        }

        uint a = h0;
        uint b = h1;
        uint c = h2;
        uint d = h3;
        uint e = h4;
        uint f = h5;
        uint g = h6;
        uint h = h7;

        for (int i = 0; i < 64; i++) {
            uint t1 = h + bsig1(e) + ch(e, f, g) + K[i] + w[i];
            uint t2 = bsig0(a) + maj(a, b, c);
            h = g;
            g = f;
            f = e;
            e = d + t1;
            d = c;
            c = b;
            b = a;
            a = t1 + t2;
        }

        h0 += a;
        h1 += b;
        h2 += c;
        h3 += d;
        h4 += e;
        h5 += f;
        h6 += g;
        h7 += h;
    }

    uint out_words[8] = { h0, h1, h2, h3, h4, h5, h6, h7 };
    for (int i = 0; i < 8; i++) {
        digest[i * 4] = (uchar)((out_words[i] >> 24) & 0xff);
        digest[i * 4 + 1] = (uchar)((out_words[i] >> 16) & 0xff);
        digest[i * 4 + 2] = (uchar)((out_words[i] >> 8) & 0xff);
        digest[i * 4 + 3] = (uchar)(out_words[i] & 0xff);
    }
}

int hash_lt_target(uchar hash[32], __global const uchar* target) {
    for (int i = 0; i < 32; i++) {
        if (hash[i] < target[i]) {
            return 1;
        }
        if (hash[i] > target[i]) {
            return 0;
        }
    }
    return 0;
}

__kernel void mine_sha256(
    __global const uchar* challenge,
    __global const uchar* miner,
    __global const uchar* target,
    ulong start_nonce,
    ulong attempt_count,
    __global uint* found_flag,
    __global ulong* found_nonce,
    __global uchar* found_hash
) {
    size_t gid = get_global_id(0);
    if ((ulong)gid >= attempt_count) {
        return;
    }

    if (*found_flag != 0) {
        return;
    }

    uchar message[72];
    for (int i = 0; i < 32; i++) {
        message[i] = challenge[i];
        message[32 + i] = miner[i];
    }

    ulong nonce = start_nonce + (ulong)gid;
    for (int i = 0; i < 8; i++) {
        message[64 + i] = (uchar)((nonce >> (8 * i)) & 0xff);
    }

    uchar digest[32];
    sha256_fixed_72(message, digest);

    if (!hash_lt_target(digest, target)) {
        return;
    }

    if (atomic_cmpxchg(found_flag, 0u, 1u) == 0u) {
        *found_nonce = nonce;
        for (int i = 0; i < 32; i++) {
            found_hash[i] = digest[i];
        }
    }
}
"#;
