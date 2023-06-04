#!/usr/bin/env python3
from typing import List
from dataclasses import dataclass
import sqlite3
import sys
import zlib


def decompress(data: bytes) -> bytes:
    if len(data) < 4:
        raise Exception(f"decompression error: missing header, length: {len(data)}")

    header, data = (data[:4], data[4:])

    expected_size = int.from_bytes(header, byteorder="big")

    data = zlib.decompress(data)

    if len(data) != expected_size:
        raise Exception(
            f"decompression error: expected {expected_size} bytes, got {len(data)}"
        )

    return data


@dataclass
class Web:
    wid: int
    created: str  # TODO datetime?
    url: str
    status: int
    response: bytes

    def __str__(self) -> str:
        return f'Web {{ id: {self.wid}, created: "{self.created}", url: "{self.url}", status: {self.status}, response.len: {len(self.response)} }}'


@dataclass
class CompressedWeb:
    wid: int
    created: str  # TODO datetime?
    url: str
    status: int
    response: bytes

    def __str__(self) -> str:
        return f'CompressedWeb {{ id: {self.wid}, created: "{self.created}", url: "{self.url}", status: {self.status}, response.len: {len(self.response)} }}'

    def decompress(self) -> Web:
        return Web(
            self.wid, self.created, self.url, self.status, decompress(self.response)
        )


def main(args: List[str]) -> int:
    if len(args) != 3:
        print(f"usage: {args[0]} <db_url> <id>")
        return 1

    db_url = args[1]
    id = int(args[2])

    pool = sqlite3.connect(db_url)

    row = pool.execute("select * from web where id = ?", (id,)).fetchone()
    if row is None:
        raise Exception(f"failed to find id: {id}")

    compressed_web = CompressedWeb(*row)
    print(f"{compressed_web}", file=sys.stderr)

    web = compressed_web.decompress()
    print(f"          {web}", file=sys.stderr)

    text = web.response.decode("utf-8")
    print(text)

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
