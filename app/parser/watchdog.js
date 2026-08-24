'use strict';

const path = require('path');
const fs = require('fs');

let cache;

module.exports.setUserDataPath = (p) => {
  cache = path.join(p, 'steam_cache/data');
};

module.exports.scan = async () => {
  try {
    let data = [];

    const files = await fs.promises.readdir(cache, { withFileTypes: true });
    for (const entry of files) {
      const file = entry.name;
      if (!entry.isFile() || !/^\d+\.db$/.test(file)) continue;
      data.push({
        appid: file.replace('.db', ''),
        source: 'Achievement Watcher : Watchdog',
        data: {
          type: 'cached',
        },
      });
    }

    return data;
  } catch (err) {
    throw err;
  }
};

module.exports.getAchievements = async (appID) => {
  return JSON.parse(await fs.promises.readFile(path.join(cache, `${appID}.db`), 'utf8'));
};
