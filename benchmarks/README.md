# Full-capacity performance evidence

This benchmark lane now runs 100 real shared MMORPG characters through the composed zone (movement and storm evaluation, not a simultaneous combat workload). Historical standalone integer-simulator results do not describe this implementation. It records:

- release-mode wall-clock cost per authoritative simulation tick;
- encoded canonical and player-scoped projection bytes.

Run it locally with:

```sh
python3 benchmarks/run.py --output-dir performance-results
```

The runner builds and executes the real `BattleRoyaleGameServerAdapter` in release mode. Each timing batch starts from a fresh deterministic 100-player match, advances 192 setup ticks outside measurement, and fails closed unless all 100 players are alive in `Combat` with active storm evaluation. It then measures 32 authoritative ticks. Five batches warm the machine and 31 batches are retained. Projection sizes are captured separately from timing by encoding one canonical snapshot and one player-scoped snapshot for every recipient from the same prepared combat state.

The output consists of `tick-cost.json`, `projection-bytes.json`, and `summary.md`. The JSON documents implement the canonical Performance Evidence 1.0.0 contract. Hosted evidence validates them against the exact Performance Evidence revision pinned in `.github/workflows/performance-evidence.yml`.

## Interpretation

Timing is advisory. Hosted runner load, CPU frequency, thermal state and virtualization can move wall-clock measurements, so ordinary correctness CI has no timing threshold and this lane does not retry until a preferred result appears.

Projection bytes are deterministic for identical source and workload. They are recorded here as performance evidence even though ordinary correctness tests separately enforce the protocol's explicit maximum-size contracts.

This benchmark does not measure WebTransport throughput, end-to-end latency, concurrent multi-match capacity or fleet performance. It also does not measure character rendering or animation, and is not evidence of 100-client readiness. Those remain separate scenarios.
