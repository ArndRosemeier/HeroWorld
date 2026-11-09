# Threads in the client

Rule (Arnd, 2026-11-09): anything that takes CPU time runs on a worker thread by
default; the main thread hands finished work to Engine. Ordering is guaranteed by
construction (one ordered queue, explicit joins, asserts), never by timing.

## What runs where

| Work | Thread | How it meets the main thread |
| --- | --- | --- |
| Single-player server: physics, combat, people, traffic, incidents (`Host`) | `hw-server` (`hw_sim::hostthread`) | one FIFO command queue; one view in flight (see below) |
| Traffic driving on, placing the cars near the camera, and what the soundscape asks of the town (cars near, district, underground) | `hw-traffic`, scoped per frame | joined in the "crowd and traffic" section; `TrafficView::plan` then `apply`; `soundscape::Survey` |
| Citizens to Engine's crowds | main | runs while `hw-traffic` works; the soundscape counts people from what it placed |
| Grass tiles and their clump lists | `hw-grass` (`grassview::Planter`) | newest ask wins; lists carry a generation, stale ones are dropped |
| Minimap tiles / the round picture | `minimap` / `minimap-paint` | one picture in flight; marks are drawn with the view it was painted for |
| Town cells, deep-underground tiles | workers, meshes `Mesh::prepare`d there | `World::spawn_prepared` on main |
| Town plans, characters, sound decoding, area rebuilds, vehicle models | workers (as before) | channels |
| Engine: animation, uploads, drawing | main (Engine's own; see ENGINE_REQUESTS.md) | |

## The server thread

`Session::Local` holds a `HostThread`. Each frame the game queues its ticks
(`Session::tick`), then calls `Session::sync`: it waits for the view asked for in the
frame before and asks for the next. So frame N shows the world after exactly the
ticks of frames up to N-1, while the server works through frame N's ticks in
parallel with the rest of the frame. Purchases, sheets, looks, taxis and debug
actions go through the same queue (`post`); debug tools that need an answer use
`call`, which runs after everything queued before it. A panic on the server thread
is passed on to the main thread. Tests: `hw_sim::hostthread::tests` (same world as
the host on its own, frame by frame; calls see everything sent before them).

The hitch log counts `server busy us` (the server thread's own time) and times
`server wait` (the main thread waiting for it).

## Measuring

`HW_BENCH=street|flight` (`scripts/bench.sh`, `bench.bat`); the report lists the
main thread's sections. In a cloud sandbox use `HW_POP=10000`, `HW_BENCH_WARMUP=100`,
low quality and a small window: the software GPU makes frames about a second, and
the full town runs out of memory there.
