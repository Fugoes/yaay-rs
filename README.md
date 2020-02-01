# `yaay-rs`

Yet Another AsYnc library for RuSt.

**It is still under development (and only under Linux on amd64 CPU)!**

## Benchmark
### spawn test
Measure the performance of spawning new task into the runtime (on my laptop with a Intel(R) Core
(TM) i7-8650U CPU @ 1.90GHz).

`yaay-rs`: `cargo run --release --bin spawn_defer_bench`
```text
test defer_bench ... bench:         263 ns/iter (+/- 76)
test spawn_bench ... bench:         330 ns/iter (+/- 84)
```
`tokio-rs`:
```text
test threaded_scheduler_local_spawn  ... bench:         450 ns/iter (+/- 58)
test threaded_scheduler_remote_spawn ... bench:         442 ns/iter (+/- 50)
```
Since this `yaay-rs` involves only 1 memory allocation while `tokio-rs` requires 2 (one for the
task structure, one for the boxed future), these result are not surprise.
 
### ping pong test
Measure the time required to listen on a local address, spawn 1000 tasks to connect to this address
, then for each connection do 10000 ping-pong (write 1 byte, and then read 1 byte for the ping
 side, read 1 byte, and then write 1 byte for the pong side) message exchange. The following results
are measured on a server with Intel(R) Xeon(R) X5670 (24 threads total).

The benchmark programs are `yaay-tests/src/mio_ping_pong.rs` for this `yaay-rs`, and
`yaay-tests/src/tokio_ping_pong.rs` for `tokio-rs`. Example usage:
```
time ./target/release/mio_ping_pong 127.0.0.1:12345 8 1
time ./target/release/tokio_ping_pong 127.0.0.1:12345 8
```
To run these programs, some system limits need to be adjusted (and these programs does no
error handling :)).

Since `yaay-rs` use didactic threads for polling IO, we run all benchmark with n worker
threads and 1 IO polling thread, while run `tokio-rs` with n worker threads. To be more fair, we
benchmark the `yaay-rs` both with and without cgroup cpu limit to n threads (n00% CPU). Each setup
is run for 10 times and take the average. The time unit is seconds.
```text
                                    User Time    Sys Time  Total Time        CPU%
                                   ==========  ==========  ==========  ==========
tokio-rs 4 threads                     58.534     198.250      64.383       398.0
yaay-rs 4 threads                      34.641     236.203      59.949       451.2
yaay-rs 4 threads (cgroup limit)       39.302     240.712      70.213       398.0
                                                                                 
tokio-rs 8 threads                     62.161     201.295      33.118       794.9
yaay-rs 8 threads                      34.204     231.521      30.416       873.0
yaay-rs 8 threads (cgroup limit)       35.398     232.284      33.613       795.9
                                                                                 
tokio-rs 12 threads                    68.834     209.257      23.368      1189.6
yaay-rs 12 threads                     35.424     224.460      20.239      1283.4
yaay-rs 12 threads (cgroup limit)      41.049     222.483      22.092      1192.1
                                                                                 
tokio-rs 16 threads                    82.657     246.455      20.767      1584.1
yaay-rs 16 threads                     39.788     249.013      17.134      1684.7
yaay-rs 16 threads (cgroup limit)      48.880     248.760      18.706      1590.4
                                                                                 
tokio-rs 20 threads                    98.952     281.916      19.277      1975.0
yaay-rs 20 threads                     41.096     274.357      15.210      2072.9
yaay-rs 20 threads (cgroup limit)      52.471     273.882      16.459      1982.1
```

## TODO
- [x] Multi-threads executor.
- [ ] Single-thread executor.
- [ ] `mio` backend (only a basic implementation now).
- [ ] Channel.
- [ ] Select on channels.

Non-goals:
- Select on futures (use channels instead).
