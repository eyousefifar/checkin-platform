# ChokePoint benchmark

For non-commercial R&D only. Do not upload this biometric data or use it as a production acceptance dataset.

```bash
scripts/chokepoint_benchmark.sh fetch
```

Build and run the real Rust engine in an isolated database:

```bash
cd apps/edge && cargo build -p pksp-cli --features ort && cd ../..
DATA_DIR=./data DATABASE_URL='sqlite:///./data/rust-bench/pksp.db?mode=rwc' \
MOCK_VISION=false REQUIRE_REAL_VISION=true CAM_IN_RTSP=rtsp://127.0.0.1:8554/demo \
CAM_IN_WEBRTC_PATH=demo ENABLE_SMART_SCENE=false MIN_FACE_PX=30 ./apps/edge/target/debug/pksp serve
```

In another terminal, enroll five identities using P1E sequence 2 / camera 2. This separates enrollment from later evaluation data and uses the dataset's stated frontal-camera choices.

```bash
ADMIN_PASSWORD=change-me scripts/chokepoint_benchmark.sh enroll
```

ChokePoint distributes 96×96 face crops, not full CCTV frames. Use it for Rust model/enrollment compatibility and recognition thresholds only. For RTSP capture, tracking, zones, direction, and attendance replay, use a consented full-frame MP4 with the existing `scripts/demo_rtsp.sh`.

`MIN_FACE_PX=30` is only the crop-benchmark setting. Recalibrate it against the real camera before using it elsewhere.

Record known-person recall and false accepts for the remaining identities. Tune thresholds only on a separate development sequence.
