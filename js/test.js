#!/usr/bin/env node
// Node.js tests for tson native bindings.
// Run: node js/test.js

const assert = require('assert').strict;

// Skip if bindings aren't built
let tson;
try {
    tson = require('../index');
} catch {
    console.log('SKIP: tson native module not built. Run: npx napi build --platform --release');
    process.exit(0);
}

const { dumps, loads, dump, load, emit } = tson;

const fs = require('fs');
const path = require('path');
const os = require('os');

let passed = 0;
let failed = 0;

function test(name, fn) {
    try {
        fn();
        passed++;
        console.log(`  [PASS] ${name}`);
    } catch (e) {
        failed++;
        console.log(`  [FAIL] ${name}: ${e.message}`);
    }
}

function jsonEq(a, b, msg) {
    assert.deepStrictEqual(JSON.parse(JSON.stringify(a)), JSON.parse(JSON.stringify(b)), msg);
}

// Round-trip tests

test('dumps/loads simple object', () => {
    const buf = dumps('{"name":"Alice","age":30}');
    assert(buf instanceof Buffer, 'returns Buffer');
    assert(buf.length > 0, 'non-empty');
    const obj = loads(buf);
    assert.strictEqual(obj.name, 'Alice');
    assert.strictEqual(obj.age, 30);
});

test('dumps/loads nested object', () => {
    const buf = dumps('{"a":{"b":1,"c":"x"}}');
    const obj = loads(buf);
    assert.strictEqual(obj.a.b, 1);
    assert.strictEqual(obj.a.c, 'x');
});

test('dumps/loads array', () => {
    const buf = dumps('[1,2,3]');
    const obj = loads(buf);
    jsonEq(obj, [1, 2, 3], 'array round-trip');
});

test('dumps/loads null and bool', () => {
    const buf = dumps('{"n":null,"t":true,"f":false}');
    const obj = loads(buf);
    assert.strictEqual(obj.n, null);
    assert.strictEqual(obj.t, true);
    assert.strictEqual(obj.f, false);
});

// File I/O tests

test('dump/load file round-trip', () => {
    const tmp = path.join(os.tmpdir(), `tson-test-${Date.now()}.tson`);
    try {
        dump('{"msg":"hello"}', tmp);
        assert(fs.existsSync(tmp), 'file exists');
        assert(fs.statSync(tmp).size > 0, 'non-empty file');
        const obj = load(tmp);
        assert.strictEqual(obj.msg, 'hello');
    } finally {
        try { fs.unlinkSync(tmp); } catch {}
    }
});

// Emit tests

test('emit object returns valid TSON', () => {
    const buf = emit({ temp: 22.5, unit: 'C' });
    assert(buf instanceof Buffer, 'emit returns Buffer');
    const obj = loads(buf);
    assert(obj !== null && typeof obj === 'object', 'result is an object');
});

test('emit array returns valid TSON', () => {
    const buf = emit([1, 2, 3]);
    const obj = loads(buf);
    jsonEq(obj, [1, 2, 3], 'emit array round-trip');
});

// Error handling

test('dumps invalid JSON throws', () => {
    assert.throws(() => dumps('not json'), /error/i);
});

// Summary

console.log(`\n  ${passed} passed, ${failed} failed${failed > 0 ? ' X' : ' V'}`);
process.exit(failed > 0 ? 1 : 0);
