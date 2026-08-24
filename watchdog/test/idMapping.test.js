'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { findGogMapping, findEpicMapping } = require('../lib/idMapping.js');

test('finds GOG mappings regardless of numeric representation', () => {
  assert.equal(findGogMapping([{ gogid: 123, steamid: '456' }], '123'), '456');
});

test('finds Epic mappings by epicid and legacy appid', () => {
  assert.equal(findEpicMapping([{ epicid: 'slug', steamid: '10' }], 'slug'), '10');
  assert.equal(findEpicMapping([{ appid: 'legacy', steamid: '11' }], 'legacy'), '11');
});

test('returns undefined when a mapping is absent', () => {
  assert.equal(findGogMapping([], 'missing'), undefined);
  assert.equal(findEpicMapping([], 'missing'), undefined);
});
