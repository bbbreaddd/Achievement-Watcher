'use strict';

function findGogMapping(cache, appID) {
  return cache.find((entry) => String(entry.gogid) === String(appID))?.steamid;
}

function findEpicMapping(cache, appID) {
  return cache.find((entry) => String(entry.epicid || entry.appid) === String(appID))?.steamid;
}

module.exports = { findGogMapping, findEpicMapping };
