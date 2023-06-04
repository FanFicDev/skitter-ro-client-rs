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

* `./venv/bin/mypy ./examples/pull_replicate.py`
* `./venv/bin/black ./examples/pull_replicate.py`

