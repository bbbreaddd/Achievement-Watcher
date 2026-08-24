'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const watcherCache = require('../parser/watchdog.js');

test('discovers numeric watchdog cache files only', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'aw-cache-'));
  try {
    const expectedRoot = path.join(root, 'steam_cache', 'data');
    fs.mkdirSync(expectedRoot, { recursive: true });
    fs.writeFileSync(path.join(expectedRoot, '12345.db'), '[]');
    fs.writeFileSync(path.join(expectedRoot, '(123).db'), '[]');
    fs.writeFileSync(path.join(expectedRoot, 'abc.db'), '[]');
    watcherCache.setUserDataPath(root);
    const result = await watcherCache.scan();
    assert.deepEqual(result.map((item) => item.appid), ['12345']);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
