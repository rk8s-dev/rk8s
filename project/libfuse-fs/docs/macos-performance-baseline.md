# macOS macFUSE Performance Baseline

`script/macfuse_bench.sh` mounts the `passthrough` example via macFUSE and
runs the fio jobs in `bench/fio.cfg`. Each invocation appends a results block
below.

## How to run

```bash
LABEL=baseline-pre-A bash project/libfuse-fs/script/macfuse_bench.sh
```

To clear macOS file cache before each workload, the script calls `sync && purge`.

## Reference notes

- Compare runs only on the same hardware; figures vary across machines.
- For metadata-heavy comparison, `meta-stat` is the relevant row.
- After the macOS roadmap stage 3 (O_PATH replacement) lands, the `meta-stat`
  numbers should improve by an order of magnitude vs. the pre-A baseline.

## Run history

(Each `script/macfuse_bench.sh` run appends a section here automatically.)

### Run `baseline-pre-A` — 2026-04-30T02:26:04.365965Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 105686.2 | 317.4 | 3293.2 |
| seqread-1m | 1821.0 | 4227.1 | 16187.4 |
| meta-stat | 55870.2 | 32.1 | 71.2 |

### Run `after-A` — 2026-04-30T03:17:10.156875Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 70000.9 | 514.0 | 4423.7 |
| seqread-1m | 1128.9 | 6586.4 | 24510.5 |
| meta-stat | 78797.6 | 21.9 | 56.1 |

### Run `after-A-run2` — 2026-04-30T03:20:44.074940Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 72412.3 | 477.2 | 4079.6 |
| seqread-1m | 1330.2 | 5603.3 | 23199.7 |
| meta-stat | 83484.1 | 21.1 | 56.1 |

### Run `after-A-eager-fallback` — 2026-04-30T03:23:58.584075Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 70980.4 | 501.8 | 4423.7 |
| seqread-1m | 1026.3 | 7438.3 | 26345.5 |
| meta-stat | 26345.9 | 63.2 | 189.4 |

### Run `after-A-eager-r2` — 2026-04-30T03:27:11.836040Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 46402.4 | 839.7 | 8224.8 |
| seqread-1m | 523.5 | 10813.4 | 147849.2 |
| meta-stat | 17778.5 | 85.5 | 436.2 |

### Run `after-A-final` — 2026-04-30T03:33:55.087817Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 72764.2 | 481.3 | 4046.8 |
| seqread-1m | 1223.1 | 6127.6 | 24772.6 |
| meta-stat | 77905.1 | 22.9 | 55.0 |

### Run `after-A-arc` — 2026-04-30T03:41:56.858274Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 69264.3 | 485.4 | 4554.8 |
| seqread-1m | 1242.8 | 5996.5 | 24248.3 |
| meta-stat | 81308.1 | 21.9 | 52.0 |

### Run `after-A-arc-r2` — 2026-04-30T03:44:14.370327Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 54935.7 | 577.5 | 5799.9 |
| seqread-1m | 1012.0 | 6914.0 | 29491.2 |
| meta-stat | 76902.5 | 23.9 | 56.1 |

### Run `after-A-arc-r3` — 2026-04-30T03:46:24.974808Z

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 73090.1 | 436.2 | 5668.9 |
| seqread-1m | 923.2 | 7438.3 | 38535.2 |
| meta-stat | 72360.6 | 22.9 | 112.1 |

### Run `ab-phase7-lazy` — 2026-04-30T15:59:59.941031Z (concurrent_mounts=0)

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 108118.0 | 309.2 | 3227.6 |
| seqread-1m | 1425.1 | 5537.8 | 21626.9 |
| meta-stat | 88566.3 | 21.9 | 40.2 |

### Run `ab-phase7-lazy` — 2026-04-30T16:10:32.725952Z (concurrent_mounts=0)

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 105516.1 | 325.6 | 3260.4 |
| seqread-1m | 1778.7 | 4145.2 | 16056.3 |
| meta-stat | 114852.3 | 15.0 | 39.2 |

### Run `ab-phase7-eager` — 2026-04-30T16:12:11.914146Z (concurrent_mounts=0)

| job | iops | p50 (µs) | p99 (µs) |
| --- | ---: | ---: | ---: |
| randread-4k | 74540.0 | 501.8 | 4079.6 |
| seqread-1m | 1412.7 | 5537.8 | 21889.0 |
| meta-stat | 41655.5 | 45.8 | 81.4 |

### A/B `ab-phase7` — 2026-04-30T16:12:11.986089Z (concurrent_mounts=0)

| job | lazy iops | eager iops | ratio |
| --- | ---: | ---: | ---: |
| meta-stat | 114852.3 | 41655.5 | 2.76x |
| randread-4k | 105516.1 | 74540.0 | 1.42x |
| seqread-1m | 1778.7 | 1412.7 | 1.26x |
