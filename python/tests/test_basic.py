"""Tests for tson Python bindings.

Run with:
    pip install maturin
    maturin build --release
    pip install target/wheels/tson-*.whl
    pytest python/tests/ -v
"""
import json
import os
import sys
import tempfile

import pytest

try:
    import tson
except ImportError:
    pytest.skip("tson module not built — run `maturin build --release && pip install target/wheels/tson-*.whl`", allow_module_level=True)


class TestDumpsLoads:
    """Compile/decompile round-trip through TSON."""

    def test_roundtrip_simple(self):
        data = '{"name":"Alice","age":30}'
        blob = tson.dumps(data)
        assert isinstance(blob, (bytes, bytearray)), "dumps returns bytes"
        result = tson.loads(blob)
        assert result["name"] == "Alice"
        assert result["age"] == 30

    def test_roundtrip_nested(self):
        data = '{"a":{"b":1,"c":"x"}}'
        blob = tson.dumps(data)
        result = tson.loads(blob)
        assert result["a"]["b"] == 1
        assert result["a"]["c"] == "x"

    def test_array(self):
        data = '[1,2,3]'
        blob = tson.dumps(data)
        result = tson.loads(blob)
        assert result == [1, 2, 3]

    def test_null_and_bool(self):
        data = '{"n":null,"t":true,"f":false}'
        blob = tson.dumps(data)
        result = tson.loads(blob)
        assert result["n"] is None
        assert result["t"] is True
        assert result["f"] is False


class TestDumpLoad:
    """File-based round-trip."""

    def test_file_roundtrip(self):
        data = '{"msg":"hello"}'
        with tempfile.NamedTemporaryFile(suffix=".tson", delete=False) as f:
            path = f.name
        try:
            tson.dump(data, path)
            assert os.path.getsize(path) > 0
            result = tson.load(path)
            assert result["msg"] == "hello"
        finally:
            os.unlink(path)


class TestEmit:
    """Direct dict/list => TSON (no JSON string)."""

    def test_emit_dict(self):
        blob = tson.emit({"temp": 22.5, "unit": "C"})
        assert isinstance(blob, (bytes, bytearray))
        result = tson.loads(blob)
        assert isinstance(result, dict)

    def test_emit_list(self):
        blob = tson.emit([1, 2, 3])
        result = tson.loads(blob)
        assert result == [1, 2, 3]


class TestCompression:
    """Verify TSON is smaller than JSON for repetitive data."""

    def test_smaller_than_json(self):
        data = json.dumps([{"id": i, "name": f"user-{i}", "status": "active"} for i in range(100)])
        tson_blob = tson.dumps(data)
        assert len(tson_blob) < len(data), "TSON should be smaller than JSON"
