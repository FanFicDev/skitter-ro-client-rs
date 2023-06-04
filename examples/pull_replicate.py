#!/usr/bin/env python3
from typing import List, Dict, Any, Optional, Callable
from dataclasses import dataclass, astuple
import base64
import functools
import inspect
import logging
import requests
import sqlite3
import sys
import time
import urllib.parse


class LoggingTimer:
    def __init__(self, name):
        self.name = name
        self.s = None

    def __enter__(self):
        self.s = time.time()

    def __exit__(self, a, b, c):
        e = time.time()
        d = e - self.s
        logging.info("  %s took %ss", self.name, f"{d:.3f}")


def trace_timing(fspec: str) -> Callable:
    def decorator(func: Callable) -> Callable:
        @functools.wraps(func)
        def wrapped(*args, **kwargs):
            with LoggingTimer(
                fspec.format(**inspect.getcallargs(func, *args, **kwargs))
            ):
                return func(*args, **kwargs)

        return wrapped

    return decorator


@dataclass
class CompressedWeb:
    wid: int
    created: str  # TODO datetime?
    url: str
    status: int
    response: bytes

    @staticmethod
    def from_json(j: Dict[str, Any]) -> "CompressedWeb":
        return CompressedWeb(
            j["id"],
            j["created"],
            j["url"],
            j["status"],
            base64.b64decode(j["response"]),
        )


class Client:
    def __init__(self, username: str, password: str) -> None:
        self.session = requests.Session()
        self.session.auth = (username, password)
        self.session.headers.update(
            {"User-Agent": f"skitter-ro-client-py/0.0.1 +{username}"}
        )
        self.base_url = "https://zst1uv23.fanfic.dev/"

    @trace_timing("fetch_max_wid")
    def fetch_max_wid(self) -> int:
        res = self.session.get(urllib.parse.urljoin(self.base_url, "v0/web/stat"))
        res.raise_for_status()
        j = res.json()
        return int(j["max_wid"])

    @trace_timing(
        "fetch_range_compressed(max_wid={max_wid}, min_wid={min_wid}, url_like={url_like})"
    )
    def fetch_range_compressed(
        self,
        min_wid: int,
        max_wid: int,
        url_like: Optional[str],
    ) -> List[CompressedWeb]:
        res = self.session.get(
            urllib.parse.urljoin(self.base_url, "v0/web/range"),
            params=[
                ("min_wid", min_wid),
                ("max_wid", max_wid),
                ("url_like", url_like),
            ],
        )
        res.raise_for_status()
        j = res.json()
        return [CompressedWeb.from_json(e) for e in j["entries"]]


def init_logging() -> None:
    logging.basicConfig(
        format="%(asctime)s\t%(levelname)s\t%(message)s",
        level=logging.DEBUG,
    )


def get_pool(db_url: str) -> sqlite3.Connection:
    conn = sqlite3.connect(db_url)
    # TODO: log statements?

    conn.execute(open("./sql/001_init.sql", "r").read())
    conn.commit()

    return conn


@trace_timing("pull(url_like={url_like})")
def pull(url_like: str, client: Client, pool: sqlite3.Connection) -> None:
    max_wid = client.fetch_max_wid()
    logging.info("fetched max_wid: %s", max_wid)
    max_wid += 1

    stored_max_wid = pool.execute("select max(id) from web").fetchone()[0]
    stored_max_wid = 0 if stored_max_wid is None else stored_max_wid
    stored_max_wid = max(149470000 - 1, stored_max_wid)

    for next_wid in range(stored_max_wid + 1, max_wid, 1000):
        target_max_wid = min(next_wid + 1000, max_wid)
        pull_block(next_wid, target_max_wid, url_like, client, pool)


@trace_timing("pull_block(url_like={url_like}, min_wid={min_wid}, max_wid={max_wid})")
def pull_block(
    min_wid: int,
    max_wid: int,
    url_like: str,
    client: Client,
    pool: sqlite3.Connection,
) -> None:
    res = client.fetch_range_compressed(
        min_wid,
        max_wid,
        url_like,
    )
    logging.info(
        "fetched block: block_span=%s count=%s",
        max_wid - min_wid,
        len(res),
    )

    pool.executemany(
        "insert into web(id, created, url, status, response) values(?, ?, ?, ?, ?)",
        [astuple(r) for r in res],
    )
    pool.commit()


def main(args: List[str]) -> int:
    init_logging()

    if len(args) != 4:
        print(f"usage: {args[0]} <user> <pass> <url_like>")
        return 1

    username = args[1]
    password = args[2]
    url_like = args[3]

    client = Client(username, password)

    db_url = "./py_web.db"
    pool = get_pool(db_url)

    while True:
        try:
            pull(url_like, client, pool)
        except KeyboardInterrupt:
            break
        time.sleep(60 - time.time() % 60)

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
