# skitter-ro-client

`skitter-ro-client` is a small rust implementation of a client for the
read-only (ro) skitter API.

## examples/simple

`example/simple` is a toy utility to showcase basic usage of the library.

## examples/pull_replicate

There are two example `pull_replicate` programs:
* `examples/pull_replicate.rs` (uses this library)
* `examples/pull_replicate.py` (pure python)

These start with an empty local sqlite db and a recent (2023-06-03) web id
(wid) and pull from the read only API every minute:

1. Fetch the remote max id.
2. Find the max id stored locally.
3. Fetch pages between the local and remote ids in 1k chunks.

These don't explicitly store the last id used in a query, so we may query over
the same id range several times due to url based filtering. For example if id
1000 is stored locally but the next matching entry will only appear at id 6000,
the loop will only advance once id 6000 is returned and all intervening ids are
queried over within the same iteration of the interval loop.

For the example programs this isn't an issue due to how dense the matching
entries are, but a different strategy should be used if replicating a smaller
subset of the data.

### examples/pull_replicate.py venv

The `pull_replicate.py` example depends on the requests library. If that
library is installed system wide it may be possible to directly run the script.
Otherwise, creating a virtual environment along with all dev dependencies can
be done in a few steps:

1. `python3 -m venv ./venv`
2. `./venv/bin/python -m pip install --upgrade pip`
3. `./venv/bin/python -m pip install -r requirements.txt`

This includes both `mypy` and `black` for type checking and auto formatting
respectively:

* `./venv/bin/mypy ./examples/*.py`
* `./venv/bin/black ./examples/*.py`

## examples/dump_id

The `dump_id` examples showcase how to get the original raw utf-8 (or bust)
response text for a given id. Response data is sent as a zlib compressed
payload prefixed by a four byte big-endian expected decompressed size. If using
Qt bindings this can be passed directly into `qUncompress` or
`CompressedWeb.decompress` shows how to decompress it using lower level zlib
bindings.

Usage examples:

* `./examples/dump_id.py ./py_web.db 149470000 | md5sum`

```
CompressedWeb { id: 149470000, created: "2023-06-03T20:59:52.632Z", url: "https://www.example.com/s/6211915/1", status: 200, response.len: 21315 }
          Web { id: 149470000, created: "2023-06-03T20:59:52.632Z", url: "https://www.example.com/s/6211915/1", status: 200, response.len: 61940 }
25b87325527ba0f58cc5d42d2d420b24  -
```

* `cargo run --release --example dump_id ./web.db 149470000 | md5sum`

```
    Finished release [optimized] target(s) in 0.09s
     Running `target/release/examples/dump_id ./web.db 149470000`
CompressedWeb { id: 149470000, created: "2023-06-03 20:59:52.632 +00:00:00", url: "https://www.example.com/s/6211915/1", status: 200, response.len: 21315 }
          Web { id: 149470000, created: "2023-06-03 20:59:52.632 +00:00:00", url: "https://www.example.com/s/6211915/1", status: 200, response.len: 61940 }
25b87325527ba0f58cc5d42d2d420b24  -
```

